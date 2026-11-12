use std::path::{Path, PathBuf};

/// All on-disk locations used by Pristine, rooted at one directory.
/// Override the root with the `PRISTINE_HOME` environment variable.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_location() -> Self {
        if let Some(p) = std::env::var_os("PRISTINE_HOME") {
            return Self::new(p);
        }
        let dirs = directories::ProjectDirs::from("dev", "pristine", "Pristine");
        Self::new(match dirs {
            Some(d) => d.data_dir().to_path_buf(),
            None => PathBuf::from(".pristine"),
        })
    }

    pub fn root(&self) -> &Path { &self.root }
    pub fn libraries(&self) -> PathBuf { self.root.join("libraries") }
    pub fn assets(&self) -> PathBuf { self.root.join("assets") }
    pub fn versions(&self) -> PathBuf { self.root.join("versions") }
    pub fn runtimes(&self) -> PathBuf { self.root.join("runtimes") }
    pub fn skins(&self) -> PathBuf { self.root.join("skins") }
    pub fn instances(&self) -> PathBuf { self.root.join("instances") }
    pub fn version_dir(&self, id: &str) -> PathBuf { self.versions().join(id) }
    pub fn version_json(&self, id: &str) -> PathBuf { self.version_dir(id).join(format!("{id}.json")) }
    pub fn client_jar(&self, id: &str) -> PathBuf { self.version_dir(id).join(format!("{id}.jar")) }
    pub fn natives_dir(&self, id: &str) -> PathBuf { self.version_dir(id).join("natives") }
    pub fn instance_dir(&self, id: &str) -> PathBuf { self.instances().join(id) }
    pub fn instance_json(&self, id: &str) -> PathBuf { self.instance_dir(id).join("instance.json") }
    pub fn game_dir(&self, id: &str) -> PathBuf { self.instance_dir(id).join("minecraft") }
}
