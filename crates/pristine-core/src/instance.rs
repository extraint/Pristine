use crate::error::{Error, Result};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    /// Directory-safe identifier derived from the name.
    pub id: String,
    pub name: String,
    pub mc_version: String,
    #[serde(default = "default_max_mem")]
    pub max_memory_mb: u32,
    #[serde(default)]
    pub java_path: Option<String>,
    #[serde(default)]
    pub extra_jvm_args: Vec<String>,
}

fn default_max_mem() -> u32 { 2048 }

pub fn slugify(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "instance".into() } else { s }
}

impl Instance {
    pub fn create(paths: &Paths, name: &str, mc_version: &str) -> Result<Instance> {
        let base = slugify(name);
        let mut id = base.clone();
        let mut n = 2;
        while paths.instance_dir(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        let inst = Instance {
            id,
            name: name.trim().to_string(),
            mc_version: mc_version.to_string(),
            max_memory_mb: default_max_mem(),
            java_path: None,
            extra_jvm_args: vec![],
        };
        std::fs::create_dir_all(paths.game_dir(&inst.id))?;
        inst.save(paths)?;
        Ok(inst)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::write(paths.instance_json(&self.id), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn load(paths: &Paths, id: &str) -> Result<Instance> {
        let raw = std::fs::read(paths.instance_json(id))
            .map_err(|_| Error::msg(format!("no such instance: {id}")))?;
        Ok(serde_json::from_slice(&raw)?)
    }

    pub fn list(paths: &Paths) -> Result<Vec<Instance>> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(paths.instances()) else { return Ok(out) };
        for entry in rd.flatten() {
            if let Some(id) = entry.file_name().to_str() {
                if let Ok(i) = Instance::load(paths, id) { out.push(i); }
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    pub fn delete(paths: &Paths, id: &str) -> Result<()> {
        std::fs::remove_dir_all(paths.instance_dir(id))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("My Cool Pack!"), "my-cool-pack");
        assert_eq!(slugify("  "), "instance");
    }

    #[test]
    fn create_list_delete() {
        let dir = std::env::temp_dir().join(format!("pristine-test-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(&dir);
        let a = Instance::create(&paths, "Test", "1.21").unwrap();
        let b = Instance::create(&paths, "Test", "1.20.1").unwrap();
        assert_eq!(a.id, "test");
        assert_eq!(b.id, "test-2");
        assert_eq!(Instance::list(&paths).unwrap().len(), 2);
        Instance::delete(&paths, "test").unwrap();
        assert_eq!(Instance::list(&paths).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
