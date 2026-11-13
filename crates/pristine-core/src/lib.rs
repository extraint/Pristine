//! Pristine core: everything a Minecraft launcher needs, with no UI attached.

pub mod accounts;
pub mod auth;
pub mod content;
pub mod download;
pub mod error;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader;
pub mod logparse;
pub mod manifest;
pub mod modpack;
pub mod modrinth;
pub mod modules;
pub mod msa;
pub mod paths;
pub mod servers;
pub mod skins;

pub use auth::Account;
pub use download::{Progress, ProgressFn};
pub use error::{Error, Result};
pub use instance::Instance;
pub use loader::Loader;
pub use launch::LaunchCommand;
pub use paths::Paths;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use accounts::{AccountStore, Config, StoredAccount};

/// Entry point holding shared state (paths + HTTP client).
#[derive(Clone)]
pub struct Pristine {
    pub paths: Paths,
    pub client: reqwest::Client,
}

impl Pristine {
    pub fn new(paths: Paths) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Pristine/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { paths, client })
    }

    pub async fn versions(&self) -> Result<manifest::VersionManifest> {
        install::fetch_manifest(&self.client).await
    }

    /// Azure client ID for Microsoft sign-in: `PRISTINE_MS_CLIENT_ID`, then the saved
    /// setting, then a value baked in at build time.
    pub fn ms_client_id(&self) -> Option<String> {
        let nonempty = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        std::env::var("PRISTINE_MS_CLIENT_ID").ok().and_then(nonempty)
            .or_else(|| Config::load(&self.paths).ms_client_id.and_then(nonempty))
            .or_else(|| option_env!("PRISTINE_MS_CLIENT_ID").map(String::from).and_then(nonempty))
    }

    pub fn set_ms_client_id(&self, id: Option<String>) -> Result<()> {
        let mut c = Config::load(&self.paths);
        c.ms_client_id = id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        c.save(&self.paths)
    }

    fn require_client_id(&self) -> Result<String> {
        self.ms_client_id().ok_or_else(|| Error::msg("Microsoft sign-in isn't set up yet: add an Azure client ID in Settings."))
    }

    /// Step 1 of Microsoft sign-in: get a code for the user to enter in their browser.
    pub async fn ms_begin(&self) -> Result<msa::DeviceCode> {
        msa::request_device_code(&self.client, &self.require_client_id()?).await
    }

    /// Step 2: wait for approval, complete the Xbox/Minecraft chain, and save the account.
    pub async fn ms_finish(&self, dc: &msa::DeviceCode, cancel: &AtomicBool) -> Result<StoredAccount> {
        let id = self.require_client_id()?;
        let tokens = msa::poll_for_token(&self.client, &id, dc, cancel).await?;
        let session = msa::minecraft_session(&self.client, &tokens.access_token).await?;
        let stored = StoredAccount {
            uuid: session.uuid.simple().to_string(),
            username: session.username,
            refresh_token: tokens.refresh_token,
            mc_token: session.access_token,
            mc_expires_at: session.expires_at,
        };
        let mut store = AccountStore::load(&self.paths);
        store.upsert(stored.clone());
        store.selected = Some(stored.uuid.clone());
        store.save(&self.paths)?;
        Ok(stored)
    }

    /// A launch-ready account for a stored Microsoft login, refreshing tokens if needed.
    pub async fn launch_account(&self, uuid: &str) -> Result<Account> {
        let mut store = AccountStore::load(&self.paths);
        let mut a = store.accounts.iter().find(|a| a.uuid == uuid).cloned()
            .ok_or_else(|| Error::msg("That account is no longer signed in"))?;
        if a.mc_expires_at <= accounts::now() + 300 {
            let id = self.require_client_id()?;
            let tokens = msa::refresh_tokens(&self.client, &id, &a.refresh_token).await?;
            let session = msa::minecraft_session(&self.client, &tokens.access_token).await?;
            a.refresh_token = tokens.refresh_token;
            a.mc_token = session.access_token;
            a.mc_expires_at = session.expires_at;
            a.username = session.username;
            store.upsert(a.clone());
            store.save(&self.paths)?;
        }
        let parsed = uuid::Uuid::parse_str(&a.uuid).map_err(|_| Error::msg("corrupt account entry"))?;
        Ok(Account::microsoft(a.username, parsed, a.mc_token))
    }

    /// Vanilla metadata with the instance's loader merged in.
    pub fn resolve_meta(&self, inst: &Instance) -> Result<manifest::VersionMeta> {
        let vanilla = install::load_version_meta(&self.paths, &inst.mc_version)?;
        loader::apply(&self.paths, inst, vanilla)
    }

    /// Download the game, loader, libraries, assets and a matching Java for an instance.
    pub async fn install_instance(&self, id: &str, progress: ProgressFn) -> Result<()> {
        let mut inst = Instance::load(&self.paths, id)?;
        let meta = install::install_version(&self.paths, &self.client, &inst.mc_version, &progress).await?;
        loader::install(&self.paths, &self.client, &mut inst, &progress).await?;
        if inst.java_path.is_none() {
            java::ensure_java(&self.paths, &self.client, meta.java_major(), &progress).await?;
        }
        Ok(())
    }

    /// Build the launch command for an installed instance.
    pub async fn prepare_launch(&self, id: &str, account: &Account, quick: Option<&launch::QuickPlay>, progress: ProgressFn) -> Result<LaunchCommand> {
        let inst = Instance::load(&self.paths, id)?;
        let meta = self.resolve_meta(&inst)?;
        let java = match &inst.java_path {
            Some(p) => p.into(),
            None => java::ensure_java(&self.paths, &self.client, meta.java_major(), &progress).await?,
        };
        std::fs::create_dir_all(self.paths.game_dir(id))?;
        Ok(launch::build_command(&self.paths, &meta, &inst, java, account, quick))
    }

    // ----- Modrinth & content -----

    pub async fn search(&self, p: &modrinth::SearchParams) -> Result<modrinth::SearchResult> {
        modrinth::search(&self.client, p).await
    }

    pub async fn install_content(
        &self, id: &str, project: &str, kind: modrinth::Kind, world: Option<&str>,
    ) -> Result<Vec<String>> {
        let inst = Instance::load(&self.paths, id)?;
        let opts = content::InstallOpts { world, module: None, explicit: true };
        content::install_project(&self.paths, &self.client, &inst, project, kind, &opts).await
    }

    pub async fn install_modpack(&self, project: &str, progress: ProgressFn) -> Result<Instance> {
        modpack::install_modpack(&self.paths, &self.client, project, &progress).await
    }

    /// Upload a saved skin to the selected Microsoft account.
    pub async fn apply_skin(&self, file: &str, slim: bool) -> Result<()> {
        let store = AccountStore::load(&self.paths);
        let uuid = store.selected.ok_or_else(|| Error::msg("Sign in with a Microsoft account to change your skin."))?;
        let acct = self.launch_account(&uuid).await?;
        let bytes = skins::read(&self.paths, file)?;
        skins::apply(&self.client, &acct.access_token, bytes, slim).await
    }

    pub async fn enable_module(&self, id: &str, module: &str) -> Result<modules::ModuleReport> {
        let mut inst = Instance::load(&self.paths, id)?;
        modules::enable(&self.paths, &self.client, &mut inst, module).await
    }

    pub fn disable_module(&self, id: &str, module: &str) -> Result<()> {
        let mut inst = Instance::load(&self.paths, id)?;
        modules::disable(&self.paths, &mut inst, module)
    }

    pub fn set_loader(&self, id: &str, new: Loader) -> Result<()> {
        let mut inst = Instance::load(&self.paths, id)?;
        loader::reset(&self.paths, &mut inst, new)
    }
}

/// Convenience: a progress callback that ignores everything.
pub fn no_progress() -> ProgressFn {
    Arc::new(|_| {})
}
