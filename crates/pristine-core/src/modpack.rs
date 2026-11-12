//! Install a Modrinth modpack (.mrpack) as a new instance.

use crate::content::safe_rel;
use crate::download::{download, download_all, Job, Progress, ProgressFn};
use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::loader::Loader;
use crate::modrinth::{self, best_version};
use crate::paths::Paths;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

#[derive(Deserialize)]
struct Index {
    name: String,
    dependencies: HashMap<String, String>,
    #[serde(default)]
    files: Vec<IndexFile>,
}

#[derive(Deserialize)]
struct IndexFile {
    path: String,
    hashes: HashMap<String, String>,
    downloads: Vec<String>,
    env: Option<HashMap<String, String>>,
}

/// The .mrpack spec only allows downloads from these hosts.
const ALLOWED_HOSTS: &[&str] = &["cdn.modrinth.com", "github.com", "raw.githubusercontent.com", "gitlab.com"];

fn host_allowed(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|r| r.split('/').next())
        .map(|h| ALLOWED_HOSTS.contains(&h))
        .unwrap_or(false)
}

pub async fn install_modpack(paths: &Paths, client: &reqwest::Client, project: &str, progress: &ProgressFn) -> Result<Instance> {
    progress(Progress { stage: "modpack", done: 0, total: 1 });
    let proj = modrinth::project(client, project).await?;
    let versions = modrinth::versions(client, &proj.id, None, None).await?;
    let v = best_version(&versions).ok_or_else(|| Error::msg("This modpack has no versions"))?;
    let f = v.primary_file().ok_or_else(|| Error::msg("This modpack has no file"))?;
    if !host_allowed(&f.url) { return Err(Error::msg("Unexpected modpack download host")); }

    let archive = paths.root().join("tmp").join(format!("{}.mrpack", v.id));
    download(client, &Job { url: f.url.clone(), dest: archive.clone(), sha1: f.hashes.sha1.clone() }).await?;

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive)?)?;
    let index: Index = {
        let mut raw = String::new();
        zip.by_name("modrinth.index.json")
            .map_err(|_| Error::msg("Not a valid .mrpack (no modrinth.index.json)"))?
            .read_to_string(&mut raw)?;
        serde_json::from_str(&raw)?
    };

    let mc = index.dependencies.get("minecraft").ok_or_else(|| Error::msg("Modpack doesn't name a Minecraft version"))?;
    let (loader, loader_version) = if let Some(v) = index.dependencies.get("fabric-loader") {
        (Loader::Fabric, Some(v.clone()))
    } else if let Some(v) = index.dependencies.get("quilt-loader") {
        (Loader::Quilt, Some(v.clone()))
    } else if index.dependencies.contains_key("forge") || index.dependencies.contains_key("neoforge") {
        return Err(Error::msg("This modpack needs Forge/NeoForge, which Pristine doesn't support yet. Fabric and Quilt packs work."));
    } else {
        (Loader::Vanilla, None)
    };

    let mut inst = Instance::create_with(paths, &index.name, mc, loader)?;
    inst.loader_version = loader_version;
    inst.save(paths)?;
    let game = paths.game_dir(&inst.id);

    let mut jobs = Vec::new();
    for file in &index.files {
        if file.env.as_ref().and_then(|e| e.get("client")).map(|s| s == "unsupported").unwrap_or(false) { continue; }
        let url = file.downloads.iter().find(|u| host_allowed(u))
            .ok_or_else(|| Error::msg(format!("No allowed download source for {}", file.path)))?;
        jobs.push(Job { url: url.clone(), dest: game.join(safe_rel(&file.path)?), sha1: file.hashes.get("sha1").cloned() });
    }
    download_all(client, jobs, "modpack files", progress).await?;

    // overrides/ and client-overrides/ are copied over the game directory.
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name() else { continue };
        let rel = match name.strip_prefix("overrides").or_else(|_| name.strip_prefix("client-overrides")) {
            Ok(r) if !r.as_os_str().is_empty() => r.to_path_buf(),
            _ => continue,
        };
        let out = game.join(&rel);
        if entry.is_dir() { std::fs::create_dir_all(&out)?; continue; }
        if let Some(p) = out.parent() { std::fs::create_dir_all(p)?; }
        std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
    }
    let _ = std::fs::remove_file(&archive);
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allowed_hosts() {
        assert!(host_allowed("https://cdn.modrinth.com/data/x/y.jar"));
        assert!(!host_allowed("https://evil.example.com/x.jar"));
        assert!(!host_allowed("http://cdn.modrinth.com/x.jar"));
        assert!(!host_allowed("https://cdn.modrinth.com.evil.com/x.jar"));
    }
}
