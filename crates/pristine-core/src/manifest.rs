//! Mojang launcher metadata: version manifest, per-version JSON, libraries, rules.

use serde::Deserialize;
use std::collections::HashMap;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

impl VersionManifest {
    pub fn find(&self, id: &str) -> Option<&VersionEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionMeta {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexRef,
    pub assets: Option<String>,
    pub downloads: Downloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
}

impl VersionMeta {
    /// Java major version this Minecraft version needs (8 for old versions).
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map(|j| j.major_version).unwrap_or(8)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JavaVersion {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: FileRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

// ---------- libraries ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    /// Legacy (pre-1.19) natives: maps OS name -> classifier.
    pub natives: Option<HashMap<String, String>>,
}

impl Library {
    pub fn is_allowed(&self) -> bool {
        rules_allow(self.rules.as_deref())
    }

    /// The jar that goes on the classpath, if any.
    pub fn artifact(&self) -> Option<&Artifact> {
        self.downloads.as_ref()?.artifact.as_ref()
    }

    /// Legacy native jar to extract into the natives directory, if any.
    pub fn native_artifact(&self) -> Option<&Artifact> {
        let key = self.natives.as_ref()?.get(os_name())?;
        let key = key.replace("${arch}", if cfg!(target_pointer_width = "32") { "32" } else { "64" });
        self.downloads.as_ref()?.classifiers.as_ref()?.get(&key)
    }
}

// ---------- rules ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

/// Mojang's OS names: `linux`, `windows`, `osx`.
pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "macos") { "osx" }
    else { "linux" }
}

/// Mojang's architecture names as they appear in rules.
pub fn current_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" }
    else if cfg!(target_arch = "x86") { "x86" }
    else { "x86_64" }
}

fn rule_matches(rule: &Rule) -> bool {
    if let Some(os) = &rule.os {
        if let Some(name) = &os.name {
            if name != os_name() { return false; }
        }
        if let Some(arch) = &os.arch {
            if arch != current_arch() { return false; }
        }
    }
    if let Some(features) = &rule.features {
        // Pristine does not enable any optional launcher features
        // (demo mode, custom resolution, quick play...), so a rule
        // requiring one to be true never matches.
        if features.values().any(|&required| required) { return false; }
    }
    true
}

/// Evaluate a rule list: no rules = allowed; otherwise the last matching rule wins.
pub fn rules_allow(rules: Option<&[Rule]>) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() { return true; }
    let mut allowed = false;
    for r in rules {
        if rule_matches(r) { allowed = r.action == "allow"; }
    }
    allowed
}

// ---------- arguments ----------

#[derive(Debug, Clone, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    pub fn resolve(&self) -> Vec<String> {
        match self {
            Argument::Plain(s) => vec![s.clone()],
            Argument::Conditional { rules, value } => {
                if !rules_allow(Some(rules)) { return vec![]; }
                match value {
                    OneOrMany::One(s) => vec![s.clone()],
                    OneOrMany::Many(v) => v.clone(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(action: &str, os: Option<&str>) -> Rule {
        Rule {
            action: action.into(),
            os: os.map(|n| OsRule { name: Some(n.into()), arch: None }),
            features: None,
        }
    }

    #[test]
    fn no_rules_allows() {
        assert!(rules_allow(None));
    }

    #[test]
    fn allow_then_disallow_other_os() {
        let rules = vec![rule("allow", None), rule("disallow", Some(os_name()))];
        assert!(!rules_allow(Some(&rules)));
    }

    #[test]
    fn allow_only_other_os_is_denied() {
        let other = if os_name() == "windows" { "linux" } else { "windows" };
        assert!(!rules_allow(Some(&[rule("allow", Some(other))])));
    }

    #[test]
    fn arch_rules_follow_cpu() {
        let mk = |arch: &str| Rule {
            action: "allow".into(),
            os: Some(OsRule { name: None, arch: Some(arch.into()) }),
            features: None,
        };
        assert!(rules_allow(Some(&[mk(current_arch())])));
        let other = if current_arch() == "arm64" { "x86_64" } else { "arm64" };
        assert!(!rules_allow(Some(&[mk(other)])));
    }

    #[test]
    fn feature_rules_never_match() {
        let r = Rule {
            action: "allow".into(),
            os: None,
            features: Some(HashMap::from([("is_demo_user".to_string(), true)])),
        };
        assert!(!rules_allow(Some(&[r])));
    }
}
