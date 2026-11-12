//! Accounts. Offline accounts work today; Microsoft sign-in is the next milestone.

use md5::{Digest, Md5};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Account {
    pub username: String,
    pub uuid: Uuid,
    pub access_token: String,
    /// "msa" for Microsoft accounts, "legacy" for offline.
    pub user_type: &'static str,
}

impl Account {
    /// A signed-in Microsoft account (the access token is a Minecraft Services token).
    pub fn microsoft(username: String, uuid: Uuid, access_token: String) -> Account {
        Account { username, uuid, access_token, user_type: "msa" }
    }

    /// Offline account with the same UUID vanilla servers derive for offline players
    /// (`UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`).
    pub fn offline(username: &str) -> Account {
        let mut h = Md5::new();
        h.update(format!("OfflinePlayer:{username}").as_bytes());
        let bytes: [u8; 16] = h.finalize().into();
        let uuid = uuid::Builder::from_md5_bytes(bytes).into_uuid();
        Account {
            username: username.to_string(),
            uuid,
            access_token: "0".into(),
            user_type: "legacy",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java() {
        // Known value of UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes())
        let a = Account::offline("Notch");
        assert_eq!(a.uuid.to_string(), "b50ad385-829d-3141-a216-7e7d7539ba7f");
    }
}
