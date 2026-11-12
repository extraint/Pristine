//! Installing and managing mods, resource packs, shaders and data packs in an instance.

use crate::download::{download, Job};
use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::modrinth::{self, best_version, loaders_for, Kind};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentEntry {
    /// Path relative to the game directory, using `/`.
    pub file: String,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub kind: Kind,
    pub version_number: String,
    /// Modules that pulled this in.
    #[serde(default)]
    pub modules: Vec<String>,
    /// Chosen by the user (so it survives disabling a module).
    #[serde(default)]
    pub explicit: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    entries: Vec<ContentEntry>,
}

fn manifest_path(paths: &Paths, id: &str) -> PathBuf { paths.instance_dir(id).join("content.json") }

fn load(paths: &Paths, id: &str) -> Manifest {
    std::fs::read(manifest_path(paths, id)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(paths: &Paths, id: &str, m: &Manifest) -> Result<()> {
    std::fs::write(manifest_path(paths, id), serde_json::to_vec_pretty(m)?)?;
    Ok(())
}

/// A relative path that can't escape its base (no `..`, no absolute paths).
pub fn safe_rel(p: &str) -> Result<PathBuf> {
    let path = Path::new(p);
    if p.is_empty() || path.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(Error::msg(format!("unsafe path: {p}")));
    }
    Ok(path.to_path_buf())
}

/// Content files may only live under these folders of the game directory.
fn check_allowed(rel: &Path) -> Result<()> {
    let top = rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or("");
    if ["mods", "resourcepacks", "shaderpacks", "saves"].contains(&top) { Ok(()) } else {
        Err(Error::msg("not a content file"))
    }
}

fn rel_string(rel: &Path) -> String {
    rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

pub struct InstallOpts<'a> {
    /// Required for data packs.
    pub world: Option<&'a str>,
    /// Tag everything installed with this module id.
    pub module: Option<&'a str>,
    pub explicit: bool,
}

/// Install a project (and its required dependencies). Returns the titles newly installed.
pub async fn install_project(
    paths: &Paths,
    client: &reqwest::Client,
    inst: &Instance,
    project: &str,
    kind: Kind,
    opts: &InstallOpts<'_>,
) -> Result<Vec<String>> {
    if kind == Kind::Modpack { return Err(Error::msg("Modpacks create a new instance")); }
    let mut manifest = load(paths, &inst.id);
    let mut queue: VecDeque<(String, bool)> = VecDeque::from([(project.to_string(), true)]);
    let mut seen = HashSet::new();
    let mut installed = Vec::new();
    let mut failure = None;

    while let Some((pref, root)) = queue.pop_front() {
        let step: Result<()> = async {
            let proj = modrinth::project(client, &pref).await?;
            if !seen.insert(proj.id.clone()) { return Ok(()); }
            let kind = if root { kind } else { Kind::Mod };

            if let Some(e) = manifest.entries.iter_mut().find(|e| e.project_id == proj.id) {
                if let Some(m) = opts.module { if !e.modules.iter().any(|x| x == m) { e.modules.push(m.into()); } }
                if root && opts.explicit { e.explicit = true; }
                return Ok(());
            }

            let loaders = loaders_for(kind, inst.loader)?;
            let versions = modrinth::versions(client, &proj.id, Some(&inst.mc_version), loaders.as_deref()).await?;
            let v = best_version(&versions).ok_or_else(|| Error::msg(format!(
                "{} has no version for Minecraft {} ({})", proj.title, inst.mc_version, inst.loader.label())))?;
            let file = v.primary_file().ok_or_else(|| Error::msg(format!("{} has no downloadable file", proj.title)))?;

            let filename = Path::new(&file.filename).file_name()
                .ok_or_else(|| Error::msg("bad file name"))?.to_string_lossy().into_owned();
            let rel = match kind {
                Kind::DataPack => {
                    let w = opts.world.ok_or_else(|| Error::msg("Pick a world for this data pack"))?;
                    safe_rel(&format!("saves/{w}/datapacks/{filename}"))?
                }
                k => safe_rel(&format!("{}/{filename}", k.folder()))?,
            };
            download(client, &Job {
                url: file.url.clone(),
                dest: paths.game_dir(&inst.id).join(&rel),
                sha1: file.hashes.sha1.clone(),
            }).await?;

            manifest.entries.push(ContentEntry {
                file: rel_string(&rel),
                project_id: proj.id.clone(),
                version_id: v.id.clone(),
                title: proj.title.clone(),
                icon_url: proj.icon_url.clone(),
                kind,
                version_number: v.version_number.clone(),
                modules: opts.module.map(|m| vec![m.to_string()]).unwrap_or_default(),
                explicit: root && opts.explicit,
            });
            installed.push(proj.title);
            for d in &v.dependencies {
                if d.dependency_type == "required" {
                    if let Some(pid) = &d.project_id { queue.push_back((pid.clone(), false)); }
                }
            }
            Ok(())
        }.await;
        if let Err(e) = step { failure = Some(e); break; }
    }
    save(paths, &inst.id, &manifest)?;
    match failure { Some(e) => Err(e), None => Ok(installed) }
}

#[derive(Debug, Serialize)]
pub struct ContentView {
    pub file: String,
    pub name: String,
    pub enabled: bool,
    pub kind: Kind,
    pub title: String,
    pub icon_url: Option<String>,
    pub version_number: Option<String>,
    pub managed: bool,
    pub modules: Vec<String>,
}

fn is_disabled(file: &str) -> bool { file.ends_with(".disabled") }

pub fn list(paths: &Paths, inst: &Instance) -> Vec<ContentView> {
    let game = paths.game_dir(&inst.id);
    let manifest = load(paths, &inst.id);
    let mut out: Vec<ContentView> = manifest.entries.iter()
        .filter(|e| game.join(&e.file).is_file())
        .map(|e| ContentView {
            file: e.file.clone(),
            name: Path::new(&e.file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            enabled: !is_disabled(&e.file),
            kind: e.kind,
            title: e.title.clone(),
            icon_url: e.icon_url.clone(),
            version_number: Some(e.version_number.clone()),
            managed: true,
            modules: e.modules.clone(),
        })
        .collect();
    let tracked: HashSet<&str> = manifest.entries.iter().map(|e| e.file.as_str()).collect();
    for kind in [Kind::Mod, Kind::ResourcePack, Kind::Shader] {
        let Ok(rd) = std::fs::read_dir(game.join(kind.folder())) else { continue };
        for f in rd.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let rel = format!("{}/{name}", kind.folder());
            if !f.path().is_file() || tracked.contains(rel.as_str()) { continue; }
            let base = name.trim_end_matches(".disabled");
            if !(base.ends_with(".jar") || base.ends_with(".zip")) { continue; }
            out.push(ContentView {
                file: rel, enabled: !is_disabled(&name), kind, title: base.to_string(), name,
                icon_url: None, version_number: None, managed: false, modules: vec![],
            });
        }
    }
    out.sort_by_key(|c| c.title.to_lowercase());
    out
}

pub fn list_worlds(paths: &Paths, inst: &Instance) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(paths.game_dir(&inst.id).join("saves")) else { return vec![] };
    let mut w: Vec<String> = rd.flatten()
        .filter(|e| e.path().join("level.dat").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    w.sort();
    w
}

fn resolve(paths: &Paths, inst: &Instance, file: &str) -> Result<(PathBuf, PathBuf)> {
    let rel = safe_rel(file)?;
    check_allowed(&rel)?;
    Ok((paths.game_dir(&inst.id).join(&rel), rel))
}

/// Enable/disable by adding or removing a `.disabled` suffix.
pub fn toggle(paths: &Paths, inst: &Instance, file: &str) -> Result<()> {
    let (path, _) = resolve(paths, inst, file)?;
    let new_file = match file.strip_suffix(".disabled") {
        Some(f) => f.to_string(),
        None => format!("{file}.disabled"),
    };
    let (new_path, _) = resolve(paths, inst, &new_file)?;
    std::fs::rename(&path, &new_path)?;
    let mut m = load(paths, &inst.id);
    if let Some(e) = m.entries.iter_mut().find(|e| e.file == file) { e.file = new_file; }
    save(paths, &inst.id, &m)
}

pub fn remove(paths: &Paths, inst: &Instance, file: &str) -> Result<()> {
    let (path, _) = resolve(paths, inst, file)?;
    let _ = std::fs::remove_file(path);
    let mut m = load(paths, &inst.id);
    m.entries.retain(|e| e.file != file);
    save(paths, &inst.id, &m)
}

/// Untag a module from every entry; delete files nothing else needs.
pub fn release_module(paths: &Paths, inst: &Instance, module: &str) -> Result<()> {
    let mut m = load(paths, &inst.id);
    let game = paths.game_dir(&inst.id);
    m.entries.retain_mut(|e| {
        if !e.modules.iter().any(|x| x == module) { return true; }
        e.modules.retain(|x| x != module);
        if e.modules.is_empty() && !e.explicit {
            let _ = std::fs::remove_file(game.join(&e.file));
            false
        } else { true }
    });
    save(paths, &inst.id, &m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths() {
        assert!(safe_rel("mods/a.jar").is_ok());
        assert!(safe_rel("../etc/passwd").is_err());
        assert!(safe_rel("/abs").is_err());
        assert!(safe_rel("mods/../../x").is_err());
        assert!(safe_rel("").is_err());
    }

    #[test]
    fn only_content_folders() {
        assert!(check_allowed(Path::new("mods/a.jar")).is_ok());
        assert!(check_allowed(Path::new("saves/w/datapacks/x.zip")).is_ok());
        assert!(check_allowed(Path::new("options.txt")).is_err());
        assert!(check_allowed(Path::new("config/x.json")).is_err());
    }

    #[test]
    fn toggle_and_release_module() {
        let dir = std::env::temp_dir().join(format!("pristine-content-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(&dir);
        let inst = Instance::create(&paths, "T", "1.21.1").unwrap();
        let game = paths.game_dir(&inst.id);
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), b"x").unwrap();
        std::fs::write(game.join("mods/b.jar"), b"x").unwrap();
        let entry = |f: &str, explicit: bool| ContentEntry {
            file: f.into(), project_id: f.into(), version_id: "v".into(), title: f.into(), icon_url: None,
            kind: Kind::Mod, version_number: "1".into(), modules: vec!["perf".into()], explicit,
        };
        save(&paths, &inst.id, &Manifest { entries: vec![entry("mods/a.jar", false), entry("mods/b.jar", true)] }).unwrap();

        toggle(&paths, &inst, "mods/a.jar").unwrap();
        assert!(game.join("mods/a.jar.disabled").is_file());
        assert!(list(&paths, &inst).iter().any(|c| c.file == "mods/a.jar.disabled" && !c.enabled));
        toggle(&paths, &inst, "mods/a.jar.disabled").unwrap();
        assert!(game.join("mods/a.jar").is_file());

        release_module(&paths, &inst, "perf").unwrap();
        assert!(!game.join("mods/a.jar").exists(), "module-only mod is removed");
        assert!(game.join("mods/b.jar").exists(), "explicitly installed mod stays");
        assert!(toggle(&paths, &inst, "../evil").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
