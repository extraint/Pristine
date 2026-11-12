//! Fabric and Quilt: resolve a loader version, download its profile, merge it over vanilla.
//! (Forge/NeoForge need installer processors and aren't supported yet.)

use crate::download::{fetch_json, ProgressFn};
use crate::error::{Error, Result};
use crate::install::download_libraries;
use crate::instance::Instance;
use crate::manifest::*;
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
}

impl Loader {
    pub fn label(self) -> &'static str {
        match self { Loader::Vanilla => "Vanilla", Loader::Fabric => "Fabric", Loader::Quilt => "Quilt" }
    }

    fn meta_base(self) -> Option<&'static str> {
        match self {
            Loader::Fabric => Some("https://meta.fabricmc.net/v2"),
            Loader::Quilt => Some("https://meta.quiltmc.org/v3"),
            Loader::Vanilla => None,
        }
    }
}

#[derive(Deserialize)]
struct LoaderProfile {
    #[serde(rename = "mainClass")]
    main_class: String,
    arguments: Option<Arguments>,
    #[serde(default)]
    libraries: Vec<Library>,
}

fn profile_path(paths: &Paths, id: &str) -> std::path::PathBuf {
    paths.instance_dir(id).join("loader.json")
}

/// Newest stable loader version that supports this Minecraft version.
pub async fn latest_loader_version(client: &reqwest::Client, loader: Loader, mc: &str) -> Result<String> {
    let base = loader.meta_base().ok_or_else(|| Error::msg("Vanilla has no loader"))?;
    let unsupported = || Error::msg(format!("{} doesn't support Minecraft {mc} yet", loader.label()));
    let list: Vec<serde_json::Value> = fetch_json(client, &format!("{base}/versions/loader/{mc}"))
        .await
        .map_err(|_| unsupported())?;
    let is_stable = |e: &&serde_json::Value| {
        let v = e["loader"]["version"].as_str().unwrap_or("");
        e["loader"]["stable"].as_bool() == Some(true)
            || (e["loader"]["stable"].is_null() && !["beta", "alpha", "pre", "rc"].iter().any(|t| v.contains(t)))
    };
    list.iter().find(is_stable).or(list.first())
        .and_then(|e| e["loader"]["version"].as_str())
        .map(String::from)
        .ok_or_else(unsupported)
}

/// Make sure the loader profile and its libraries are on disk.
pub async fn install(paths: &Paths, client: &reqwest::Client, inst: &mut Instance, progress: &ProgressFn) -> Result<()> {
    let Some(base) = inst.loader.meta_base() else { return Ok(()) };
    let version = match &inst.loader_version {
        Some(v) => v.clone(),
        None => {
            let v = latest_loader_version(client, inst.loader, &inst.mc_version).await?;
            inst.loader_version = Some(v.clone());
            inst.save(paths)?;
            v
        }
    };
    let file = profile_path(paths, &inst.id);
    if !file.is_file() {
        let url = format!("{base}/versions/loader/{}/{version}/profile/json", inst.mc_version);
        let bytes = client.get(&url).send().await?.error_for_status()?.bytes().await?;
        std::fs::write(&file, &bytes)?;
    }
    let profile: LoaderProfile = serde_json::from_slice(&std::fs::read(&file)?)?;
    download_libraries(paths, client, &profile.libraries, progress).await?;
    Ok(())
}

/// Forget the downloaded profile (after changing loader or loader version).
pub fn reset(paths: &Paths, inst: &mut Instance, loader: Loader) -> Result<()> {
    inst.loader = loader;
    inst.loader_version = None;
    let _ = std::fs::remove_file(profile_path(paths, &inst.id));
    inst.save(paths)
}

/// Vanilla metadata with the instance's loader layered on top.
pub fn apply(paths: &Paths, inst: &Instance, mut meta: VersionMeta) -> Result<VersionMeta> {
    if inst.loader == Loader::Vanilla { return Ok(meta); }
    let raw = std::fs::read(profile_path(paths, &inst.id))
        .map_err(|_| Error::msg("Loader isn't installed yet"))?;
    let p: LoaderProfile = serde_json::from_slice(&raw)?;
    let mut libs = p.libraries;
    libs.extend(std::mem::take(&mut meta.libraries));
    meta.libraries = libs;
    meta.main_class = p.main_class;
    if let Some(pa) = p.arguments {
        let args = meta.arguments.get_or_insert_with(Arguments::default);
        args.jvm.extend(pa.jvm);
        args.game.extend(pa.game);
    }
    Ok(meta)
}
