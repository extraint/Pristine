//! Downloads everything a vanilla Minecraft version needs to run.

use crate::download::{download, download_all, fetch_json, Job, Progress, ProgressFn};
use crate::error::{Error, Result};
use crate::manifest::*;
use crate::paths::Paths;
use std::path::Path;

pub async fn fetch_manifest(client: &reqwest::Client) -> Result<VersionManifest> {
    fetch_json(client, VERSION_MANIFEST_URL).await
}

/// Load an installed version's JSON from disk.
pub fn load_version_meta(paths: &Paths, id: &str) -> Result<VersionMeta> {
    let raw = std::fs::read(paths.version_json(id))
        .map_err(|_| Error::msg(format!("Minecraft {id} is not installed")))?;
    Ok(serde_json::from_slice(&raw)?)
}

pub async fn install_version(
    paths: &Paths,
    client: &reqwest::Client,
    version_id: &str,
    progress: &ProgressFn,
) -> Result<VersionMeta> {
    progress(Progress { stage: "version manifest", done: 0, total: 1 });
    let manifest = fetch_manifest(client).await?;
    let entry = manifest
        .find(version_id)
        .ok_or_else(|| Error::msg(format!("unknown Minecraft version: {version_id}")))?;

    // Version JSON
    download(client, &Job {
        url: entry.url.clone(),
        dest: paths.version_json(version_id),
        sha1: Some(entry.sha1.clone()),
    }).await?;
    let meta = load_version_meta(paths, version_id)?;

    // Client jar
    download(client, &Job {
        url: meta.downloads.client.url.clone(),
        dest: paths.client_jar(version_id),
        sha1: Some(meta.downloads.client.sha1.clone()),
    }).await?;

    // Libraries (+ legacy natives)
    let native_jars = download_libraries(paths, client, &meta.libraries, progress).await?;

    // Assets
    let index_path = paths.assets().join("indexes").join(format!("{}.json", meta.asset_index.id));
    download(client, &Job {
        url: meta.asset_index.url.clone(),
        dest: index_path.clone(),
        sha1: Some(meta.asset_index.sha1.clone()),
    }).await?;
    let index: AssetIndex = serde_json::from_slice(&std::fs::read(&index_path)?)?;
    let mut seen = std::collections::HashSet::new();
    let jobs: Vec<Job> = index.objects.values()
        .filter(|o| seen.insert(o.hash.clone()))
        .map(|o| {
            let prefix = &o.hash[..2];
            Job {
                url: format!("{RESOURCES_URL}/{prefix}/{}", o.hash),
                dest: paths.assets().join("objects").join(prefix).join(&o.hash),
                sha1: Some(o.hash.clone()),
            }
        })
        .collect();
    download_all(client, jobs, "assets", progress).await?;

    // Natives
    let natives = paths.natives_dir(version_id);
    std::fs::create_dir_all(&natives)?;
    for jar in native_jars {
        extract_natives(&jar, &natives)?;
    }
    Ok(meta)
}

fn opt_hash(h: &str) -> Option<String> {
    Some(h.to_string()).filter(|h| !h.is_empty())
}

/// Download every allowed library; returns legacy native jars to extract.
pub async fn download_libraries(
    paths: &Paths,
    client: &reqwest::Client,
    libs: &[Library],
    progress: &ProgressFn,
) -> Result<Vec<std::path::PathBuf>> {
    let mut jobs = Vec::new();
    let mut native_jars = Vec::new();
    for lib in libs.iter().filter(|l| l.is_allowed()) {
        if let Some(a) = lib.artifact() {
            jobs.push(Job { url: a.url.clone(), dest: paths.libraries().join(&a.path), sha1: opt_hash(&a.sha1) });
        }
        if let Some(a) = lib.native_artifact() {
            let dest = paths.libraries().join(&a.path);
            jobs.push(Job { url: a.url.clone(), dest: dest.clone(), sha1: opt_hash(&a.sha1) });
            native_jars.push(dest);
        }
    }
    download_all(client, jobs, "libraries", progress).await?;
    Ok(native_jars)
}

/// Extract shared libraries from a legacy natives jar, flattened into `dest`.
fn extract_natives(jar: &Path, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(jar)?)?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() { continue; }
        let Some(name) = f.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
        let lower = name.to_string_lossy().to_lowercase();
        if !(lower.ends_with(".so") || lower.ends_with(".dll") || lower.ends_with(".dylib") || lower.ends_with(".jnilib")) {
            continue;
        }
        let mut out = std::fs::File::create(dest.join(name))?;
        std::io::copy(&mut f, &mut out)?;
    }
    Ok(())
}
