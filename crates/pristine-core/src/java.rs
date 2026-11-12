//! Finds a suitable Java, or downloads a JRE from Adoptium (Temurin).

use crate::download::{download, Job, Progress, ProgressFn};
use crate::error::{Error, Result};
use crate::paths::Paths;
use std::path::{Path, PathBuf};
use std::process::Command;

fn exe() -> &'static str { if cfg!(windows) { "java.exe" } else { "java" } }

/// Parse the major version out of `java -version` output.
pub fn parse_major(output: &str) -> Option<u32> {
    let start = output.find('"')? + 1;
    let rest = &output[start..];
    let ver = &rest[..rest.find('"')?];
    let mut parts = ver.split(|c: char| c == '.' || c == '_' || c == '-' || c == '+');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 { parts.next()?.parse().ok() } else { Some(first) }
}

pub fn java_major(java: &Path) -> Option<u32> {
    let out = Command::new(java).arg("-version").output().ok()?;
    parse_major(&String::from_utf8_lossy(&out.stderr))
}

fn find_java_in(root: &Path, depth: u32) -> Option<PathBuf> {
    let direct = root.join("bin").join(exe());
    if direct.is_file() { return Some(direct); }
    if depth == 0 { return None; }
    for e in std::fs::read_dir(root).ok()?.flatten() {
        if e.path().is_dir() {
            if let Some(p) = find_java_in(&e.path(), depth - 1) { return Some(p); }
        }
    }
    None
}

fn managed_dir(paths: &Paths, major: u32) -> PathBuf { paths.runtimes().join(format!("java-{major}")) }

/// Return a `java` executable of exactly `major` (managed runtime, then system, then download).
pub async fn ensure_java(
    paths: &Paths,
    client: &reqwest::Client,
    major: u32,
    progress: &ProgressFn,
) -> Result<PathBuf> {
    let dir = managed_dir(paths, major);
    if let Some(j) = find_java_in(&dir, 4) { return Ok(j); }

    let mut candidates = vec![];
    if let Some(home) = std::env::var_os("JAVA_HOME") { candidates.push(PathBuf::from(home).join("bin").join(exe())); }
    candidates.push(PathBuf::from("java"));
    for c in candidates {
        if java_major(&c) == Some(major) { return Ok(c); }
    }

    install_adoptium(paths, client, major, progress).await?;
    find_java_in(&dir, 4).ok_or_else(|| Error::msg("downloaded Java runtime has no bin/java"))
}

async fn install_adoptium(paths: &Paths, client: &reqwest::Client, major: u32, progress: &ProgressFn) -> Result<()> {
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let url = format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse");
    let archive = paths.runtimes().join(format!("java-{major}.archive"));
    progress(Progress { stage: "java runtime", done: 0, total: 1 });
    download(client, &Job { url, dest: archive.clone(), sha1: None }).await?;

    let dest = managed_dir(paths, major);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest)?;
    let (a, d) = (archive.clone(), dest.clone());
    let res = tokio::task::spawn_blocking(move || extract(&a, &d))
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    let _ = std::fs::remove_file(&archive);
    if res.is_err() { let _ = std::fs::remove_dir_all(&dest); }
    progress(Progress { stage: "java runtime", done: 1, total: 1 });
    res
}

fn strip_first(p: &Path) -> PathBuf { p.components().skip(1).collect() }

fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let mut magic = [0u8; 2];
    {
        use std::io::Read;
        std::fs::File::open(archive)?.read_exact(&mut magic)?;
    }
    if magic == *b"PK" {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let Some(rel) = f.enclosed_name().map(|p| strip_first(&p)) else { continue };
            if rel.as_os_str().is_empty() { continue; }
            let out = dest.join(rel);
            if f.is_dir() { std::fs::create_dir_all(&out)?; continue; }
            if let Some(p) = out.parent() { std::fs::create_dir_all(p)?; }
            std::io::copy(&mut f, &mut std::fs::File::create(&out)?)?;
        }
    } else {
        let gz = flate2::read::GzDecoder::new(std::fs::File::open(archive)?);
        let mut tar = tar::Archive::new(gz);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let rel = strip_first(&entry.path()?);
            if rel.as_os_str().is_empty() || rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                continue;
            }
            let out = dest.join(rel);
            if let Some(p) = out.parent() { std::fs::create_dir_all(p)?; }
            entry.unpack(&out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_major("openjdk version \"17.0.8\" 2023-07-18"), Some(17));
        assert_eq!(parse_major("java version \"1.8.0_382\""), Some(8));
        assert_eq!(parse_major("openjdk version \"21\" 2023-09-19"), Some(21));
        assert_eq!(parse_major("garbage"), None);
    }
}
