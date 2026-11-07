use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerEntry {
    pub name: String,
    pub uuid: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("json encoding failed: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default)]
pub struct PlayerManager;

impl PlayerManager {
    pub fn new() -> Self {
        Self
    }

    pub fn read_whitelist(&self, path: impl AsRef<Path>) -> Result<Vec<PlayerEntry>, PlayerError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path)?;
        let entries = serde_json::from_str(&content)?;
        Ok(entries)
    }

    pub fn write_whitelist(
        &self,
        path: impl AsRef<Path>,
        entries: &[PlayerEntry],
    ) -> Result<(), PlayerError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string(entries)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn read_banlist(&self, path: impl AsRef<Path>) -> Result<Vec<PlayerEntry>, PlayerError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path)?;
        let entries = serde_json::from_str(&content)?;
        Ok(entries)
    }

    pub fn write_banlist(
        &self,
        path: impl AsRef<Path>,
        entries: &[PlayerEntry],
    ) -> Result<(), PlayerError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string(entries)?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn whitelist_and_ban_entries_are_written_as_json() {
        let root = tempdir().unwrap();
        let whitelist_path = root.path().join("whitelist.json");
        let ban_path = root.path().join("banned-players.json");

        let entries = vec![PlayerEntry {
            name: "Steve".to_string(),
            uuid: "abc-123".to_string(),
        }];

        let pm = PlayerManager::new();
        assert!(pm.read_whitelist(&whitelist_path).unwrap().is_empty());
        assert!(pm.read_banlist(&ban_path).unwrap().is_empty());

        pm.write_whitelist(&whitelist_path, &entries).unwrap();
        pm.write_banlist(&ban_path, &entries).unwrap();

        let read_whitelist = pm.read_whitelist(&whitelist_path).unwrap();
        let read_bans = pm.read_banlist(&ban_path).unwrap();
        assert_eq!(read_whitelist, entries);
        assert_eq!(read_bans, entries);

        let whitelist = fs::read_to_string(&whitelist_path).unwrap();
        let bans = fs::read_to_string(&ban_path).unwrap();
        assert!(whitelist.contains("\"name\":\"Steve\""));
        assert!(bans.contains("\"uuid\":\"abc-123\""));
    }
}
