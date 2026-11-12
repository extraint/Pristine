//! Pristine core: everything a Minecraft launcher needs, with no UI attached.

pub mod auth;
pub mod download;
pub mod error;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod manifest;
pub mod paths;

pub use auth::Account;
pub use download::{Progress, ProgressFn};
pub use error::{Error, Result};
pub use instance::Instance;
pub use launch::LaunchCommand;
pub use paths::Paths;

use std::sync::Arc;

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

    /// Download the game, libraries, assets and a matching Java for an instance.
    pub async fn install_instance(&self, id: &str, progress: ProgressFn) -> Result<()> {
        let inst = Instance::load(&self.paths, id)?;
        let meta = install::install_version(&self.paths, &self.client, &inst.mc_version, &progress).await?;
        if inst.java_path.is_none() {
            java::ensure_java(&self.paths, &self.client, meta.java_major(), &progress).await?;
        }
        Ok(())
    }

    /// Build the launch command for an installed instance.
    pub async fn prepare_launch(&self, id: &str, account: &Account, progress: ProgressFn) -> Result<LaunchCommand> {
        let inst = Instance::load(&self.paths, id)?;
        let meta = install::load_version_meta(&self.paths, &inst.mc_version)?;
        let java = match &inst.java_path {
            Some(p) => p.into(),
            None => java::ensure_java(&self.paths, &self.client, meta.java_major(), &progress).await?,
        };
        std::fs::create_dir_all(self.paths.game_dir(id))?;
        Ok(launch::build_command(&self.paths, &meta, &inst, java, account))
    }
}

/// Convenience: a progress callback that ignores everything.
pub fn no_progress() -> ProgressFn {
    Arc::new(|_| {})
}
