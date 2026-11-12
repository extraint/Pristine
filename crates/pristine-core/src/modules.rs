//! Client modules: curated bundles of Modrinth mods (plus launcher-side tuning) that can be
//! switched on per instance. This is what turns Pristine from a launcher into a client.

use crate::content::{self, InstallOpts};
use crate::error::{Error, Result};
use crate::instance::Instance;
use crate::loader::Loader;
use crate::modrinth::Kind;
use crate::paths::Paths;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ModuleDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Modrinth project slugs installed by this module.
    pub projects: &'static [&'static str],
    /// JVM flags added at launch.
    pub jvm: &'static [&'static str],
}

pub const MODULES: &[ModuleDef] = &[
    ModuleDef {
        id: "performance",
        name: "Performance",
        description: "Higher FPS and smoother chunk loading: Sodium, Lithium, FerriteCore, ImmediatelyFast, Entity Culling.",
        projects: &["sodium", "lithium", "ferrite-core", "immediatelyfast", "entityculling", "sodium-extra", "reeses-sodium-options"],
        jvm: &[],
    },
    ModuleDef {
        id: "visuals",
        name: "Visuals",
        description: "Shader support with Iris, connected textures with Continuity, and a smooth zoom key.",
        projects: &["iris", "continuity", "zoomify"],
        jvm: &[],
    },
    ModuleDef {
        id: "quality-of-life",
        name: "Quality of life",
        description: "Mod Menu, Mouse Tweaks, AppleSkin food info, and No Chat Reports.",
        projects: &["modmenu", "mouse-tweaks", "appleskin", "no-chat-reports"],
        jvm: &[],
    },
    ModuleDef {
        id: "navigation",
        name: "Navigation",
        description: "Xaero's Minimap and World Map.",
        projects: &["xaeros-minimap", "xaeros-world-map"],
        jvm: &[],
    },
    ModuleDef {
        id: "jvm-tuning",
        name: "Tuned JVM",
        description: "G1 garbage collector flags that cut stutter. Works on every instance, including vanilla.",
        projects: &[],
        jvm: &["-XX:+UseG1GC", "-XX:+ParallelRefProcEnabled", "-XX:MaxGCPauseMillis=200", "-XX:+DisableExplicitGC"],
    },
];

pub fn get(id: &str) -> Option<&'static ModuleDef> { MODULES.iter().find(|m| m.id == id) }

pub fn jvm_flags(enabled: &[String]) -> Vec<String> {
    MODULES.iter()
        .filter(|m| enabled.iter().any(|e| e == m.id))
        .flat_map(|m| m.jvm.iter().map(|s| s.to_string()))
        .collect()
}

#[derive(Debug, Serialize)]
pub struct ModuleReport {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
}

pub async fn enable(paths: &Paths, client: &reqwest::Client, inst: &mut Instance, id: &str) -> Result<ModuleReport> {
    let def = get(id).ok_or_else(|| Error::msg(format!("unknown module {id}")))?;
    if !def.projects.is_empty() && inst.loader == Loader::Vanilla {
        return Err(Error::msg(format!("{} needs Fabric or Quilt. Change this instance's loader in its settings.", def.name)));
    }
    let mut report = ModuleReport { installed: vec![], skipped: vec![] };
    for slug in def.projects {
        let opts = InstallOpts { world: None, module: Some(id), explicit: false };
        match content::install_project(paths, client, inst, slug, Kind::Mod, &opts).await {
            Ok(mut names) => report.installed.append(&mut names),
            Err(e) => report.skipped.push(format!("{slug}: {e}")),
        }
    }
    if !def.projects.is_empty() && report.installed.is_empty() && report.skipped.len() == def.projects.len() {
        return Err(Error::msg(format!("Nothing in {} is available for Minecraft {} yet.", def.name, inst.mc_version)));
    }
    if !inst.modules.iter().any(|m| m == id) { inst.modules.push(id.to_string()); }
    inst.save(paths)?;
    Ok(report)
}

pub fn disable(paths: &Paths, inst: &mut Instance, id: &str) -> Result<()> {
    content::release_module(paths, inst, id)?;
    inst.modules.retain(|m| m != id);
    inst.save(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jvm_flags_only_for_enabled() {
        assert!(jvm_flags(&[]).is_empty());
        assert!(jvm_flags(&["jvm-tuning".into()]).contains(&"-XX:+UseG1GC".to_string()));
        assert!(jvm_flags(&["performance".into()]).is_empty());
    }

    #[test]
    fn module_ids_unique() {
        let mut ids: Vec<_> = MODULES.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODULES.len());
    }
}
