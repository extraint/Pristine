//! Persistent Microsoft accounts and app config.

use crate::error::Result;
use crate::paths::Paths;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Write a file atomically; on unix the file is private to the user (0600).
fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(data)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub uuid: String,
    pub username: String,
    pub refresh_token: String,
    pub mc_token: String,
    pub mc_expires_at: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AccountStore {
    #[serde(default)]
    pub accounts: Vec<StoredAccount>,
    /// UUID of the selected Microsoft account; `None` means offline.
    #[serde(default)]
    pub selected: Option<String>,
}

impl AccountStore {
    fn file(paths: &Paths) -> std::path::PathBuf { paths.root().join("accounts.json") }

    pub fn load(paths: &Paths) -> AccountStore {
        std::fs::read(Self::file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(paths.root())?;
        write_private(&Self::file(paths), &serde_json::to_vec_pretty(self)?)
    }

    pub fn upsert(&mut self, acct: StoredAccount) {
        match self.accounts.iter_mut().find(|a| a.uuid == acct.uuid) {
            Some(existing) => *existing = acct,
            None => self.accounts.push(acct),
        }
    }

    pub fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.selected.as_deref() == Some(uuid) { self.selected = None; }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ms_client_id: Option<String>,
}

impl Config {
    fn file(paths: &Paths) -> std::path::PathBuf { paths.root().join("config.json") }

    pub fn load(paths: &Paths) -> Config {
        std::fs::read(Self::file(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(paths.root())?;
        std::fs::write(Self::file(paths), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(id: &str, name: &str) -> StoredAccount {
        StoredAccount { uuid: id.into(), username: name.into(), refresh_token: "r".into(), mc_token: "t".into(), mc_expires_at: 1 }
    }

    #[test]
    fn store_roundtrip_upsert_remove() {
        let dir = std::env::temp_dir().join(format!("pristine-acct-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(&dir);
        let mut s = AccountStore::load(&paths);
        assert!(s.accounts.is_empty());
        s.upsert(acct("a", "Alex"));
        s.upsert(acct("a", "AlexRenamed"));
        s.upsert(acct("b", "Bo"));
        s.selected = Some("a".into());
        s.save(&paths).unwrap();

        let mut s = AccountStore::load(&paths);
        assert_eq!(s.accounts.len(), 2);
        assert_eq!(s.accounts[0].username, "AlexRenamed");
        s.remove("a");
        assert_eq!(s.selected, None);
        assert_eq!(s.accounts.len(), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("accounts.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "accounts.json must not be group/world accessible");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
