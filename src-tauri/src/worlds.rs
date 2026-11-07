use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("world not found: {0}")]
    NotFound(String),
    #[error("invalid server.properties entry: {0}")]
    InvalidProperty(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldConfig {
    pub properties: BTreeMap<String, String>,
}

impl WorldConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, WorldError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let mut properties = BTreeMap::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let Some((key, value)) = trimmed.split_once('=') else {
                return Err(WorldError::InvalidProperty(trimmed.to_string()));
            };

            properties.insert(key.trim().to_string(), value.trim().to_string());
        }

        Ok(Self { properties })
    }

    pub fn write_to_file(&self, path: impl AsRef<Path>) -> Result<(), WorldError> {
        let mut lines = Vec::new();
        for (key, value) in &self.properties {
            lines.push(format!("{key}={value}"));
        }

        fs::write(path, lines.join("\n") + "\n")?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct WorldManager;

impl WorldManager {
    pub fn new() -> Self {
        Self
    }

    pub fn read_server_properties(
        &self,
        instance_dir: impl AsRef<Path>,
    ) -> Result<WorldConfig, WorldError> {
        let path = instance_dir.as_ref().join("server.properties");
        WorldConfig::from_file(path)
    }

    pub fn write_server_properties(
        &self,
        instance_dir: impl AsRef<Path>,
        config: &WorldConfig,
    ) -> Result<(), WorldError> {
        let path = instance_dir.as_ref().join("server.properties");
        config.write_to_file(path)
    }

    pub fn set_property(
        &self,
        instance_dir: impl AsRef<Path>,
        key: &str,
        value: &str,
    ) -> Result<(), WorldError> {
        let path = instance_dir.as_ref().join("server.properties");
        let mut config = if path.exists() {
            WorldConfig::from_file(&path)?
        } else {
            WorldConfig::default()
        };
        config.properties.insert(key.to_string(), value.to_string());
        config.write_to_file(path)
    }

    pub fn import_world(
        &self,
        instance_dir: impl AsRef<Path>,
        world_path: impl AsRef<Path>,
    ) -> Result<PathBuf, WorldError> {
        let instance_dir = instance_dir.as_ref();
        let world_path = world_path.as_ref();
        if !world_path.exists() {
            return Err(WorldError::NotFound(world_path.display().to_string()));
        }

        let target = instance_dir.join("world");
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }

        fs::create_dir_all(instance_dir)?;
        fs::rename(world_path, &target)?;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn every_server_properties_field_round_trips_through_the_editor() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(&instance_dir).unwrap();
        fs::write(
            instance_dir.join("server.properties"),
            "level-name=world\nallow-flight=false\n",
        )
        .unwrap();

        let manager = WorldManager::new();
        let mut config = manager.read_server_properties(&instance_dir).unwrap();
        config
            .properties
            .insert("difficulty".to_string(), "hard".to_string());
        manager
            .write_server_properties(&instance_dir, &config)
            .unwrap();

        let written = manager.read_server_properties(&instance_dir).unwrap();
        assert_eq!(written.properties.get("level-name").unwrap(), "world");
        assert_eq!(written.properties.get("difficulty").unwrap(), "hard");
    }

    #[test]
    fn importing_a_world_path_sets_the_instance_world() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        let world_source = root.path().join("source-world");
        fs::create_dir_all(&instance_dir).unwrap();
        fs::create_dir_all(&world_source).unwrap();
        fs::write(world_source.join("level.dat"), "world").unwrap();

        let manager = WorldManager::new();
        let imported = manager.import_world(&instance_dir, &world_source).unwrap();

        assert_eq!(imported, instance_dir.join("world"));
        assert!(instance_dir.join("world").join("level.dat").exists());
    }
}
