use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("instance does not exist: {0}")]
    NotFound(String),
    #[error("instance name already exists: {0}")]
    DuplicateName(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Instance {
    pub id: i64,
    pub name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub path: PathBuf,
    pub world: PathBuf,
}

pub struct InstanceStore {
    conn: Connection,
}

impl InstanceStore {
    pub fn open(
        db_path: impl AsRef<Path>,
        root_dir: impl AsRef<Path>,
    ) -> Result<Self, InstanceError> {
        let root_dir = root_dir.as_ref();
        fs::create_dir_all(root_dir)?;

        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS instances (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                minecraft_version TEXT NOT NULL,
                loader TEXT NOT NULL,
                path TEXT NOT NULL,
                world TEXT NOT NULL
            )",
            (),
        )?;

        Ok(Self { conn })
    }

    pub fn create_instance(
        &mut self,
        name: &str,
        minecraft_version: &str,
        loader: &str,
        instance_dir: impl AsRef<Path>,
    ) -> Result<Instance, InstanceError> {
        let instance_dir = instance_dir.as_ref();
        fs::create_dir_all(instance_dir)?;
        let world_dir = instance_dir.join("world");
        fs::create_dir_all(&world_dir)?;

        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM instances WHERE name = ?1",
                params![name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();

        if exists {
            return Err(InstanceError::DuplicateName(name.to_string()));
        }

        let path = instance_dir.to_string_lossy().to_string();
        let world = world_dir.to_string_lossy().to_string();

        self.conn.execute(
            "INSERT INTO instances (name, minecraft_version, loader, path, world) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, minecraft_version, loader, path, world],
        )?;

        let id = self.conn.last_insert_rowid();
        Ok(Instance {
            id,
            name: name.to_string(),
            minecraft_version: minecraft_version.to_string(),
            loader: loader.to_string(),
            path: instance_dir.to_path_buf(),
            world: world_dir,
        })
    }

    pub fn list_instances(&self) -> Result<Vec<Instance>, InstanceError> {
        let mut rows = self.conn.prepare(
            "SELECT id, name, minecraft_version, loader, path, world FROM instances ORDER BY id ASC",
        )?;

        let instances = rows
            .query_map([], |row| {
                Ok(Instance {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    minecraft_version: row.get(2)?,
                    loader: row.get(3)?,
                    path: PathBuf::from(row.get::<_, String>(4)?),
                    world: PathBuf::from(row.get::<_, String>(5)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(instances)
    }

    pub fn duplicate_instance(
        &mut self,
        source_dir: impl AsRef<Path>,
        new_name: &str,
    ) -> Result<Instance, InstanceError> {
        let source_dir = source_dir.as_ref();
        if !source_dir.exists() {
            return Err(InstanceError::NotFound(source_dir.display().to_string()));
        }

        let parent = source_dir.parent().unwrap_or_else(|| Path::new("."));
        let destination = parent.join(new_name);
        self.copy_dir_without_world(source_dir, &destination)?;

        let instance = self.create_instance(new_name, "1.20.1", "fabric", &destination)?;
        let _ = fs::remove_dir_all(destination.join("world"));
        Ok(instance)
    }

    fn copy_dir_without_world(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), InstanceError> {
        if destination.exists() {
            fs::remove_dir_all(destination)?;
        }
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();

            if file_name == "world" {
                continue;
            }

            let target = destination.join(file_name);
            if entry.file_type()?.is_dir() {
                self.copy_dir_without_world(&path, &target)?;
            } else {
                fs::copy(&path, &target)?;
            }
        }

        Ok(())
    }

    pub fn delete_instance(&mut self, name: &str) -> Result<(), InstanceError> {
        let instance = self
            .conn
            .query_row(
                "SELECT path FROM instances WHERE name = ?1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| InstanceError::NotFound(name.to_string()))?;

        let path = PathBuf::from(instance);
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }

        self.conn
            .execute("DELETE FROM instances WHERE name = ?1", params![name])?;
        Ok(())
    }

    /// Duplicates an existing stored instance by name, preserving its version and loader while omitting `world/`.
    pub fn duplicate_named_instance(
        &mut self,
        source_name: &str,
        new_name: &str,
    ) -> Result<Instance, InstanceError> {
        let (mc_version, loader, source_path_str) = self
            .conn
            .query_row(
                "SELECT minecraft_version, loader, path FROM instances WHERE name = ?1",
                params![source_name],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| InstanceError::NotFound(source_name.to_string()))?;

        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM instances WHERE name = ?1",
                params![new_name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(InstanceError::DuplicateName(new_name.to_string()));
        }

        let source_path = PathBuf::from(source_path_str);
        if !source_path.exists() {
            return Err(InstanceError::NotFound(source_path.display().to_string()));
        }

        let parent = source_path.parent().unwrap_or_else(|| Path::new("."));
        let destination = parent.join(new_name);
        self.copy_dir_without_world(&source_path, &destination)?;

        let instance = self.create_instance(new_name, &mc_version, &loader, &destination)?;
        let _ = fs::remove_dir_all(destination.join("world"));
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn metadata_persists_across_restarts() {
        let root = tempdir().unwrap();
        let db_path = root.path().join("instances.db");
        let dir = root.path().join("servers");
        fs::create_dir_all(&dir).unwrap();

        let mut store = InstanceStore::open(&db_path, &dir).unwrap();
        let instance = store
            .create_instance("survival", "1.20.1", "fabric", dir.join("survival"))
            .unwrap();

        assert_eq!(instance.name, "survival");

        let reopened = InstanceStore::open(&db_path, &dir).unwrap();
        let loaded = reopened.list_instances().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "survival");
    }

    #[test]
    fn duplicate_copies_everything_except_world() {
        let root = tempdir().unwrap();
        let dir = root.path().join("servers");
        fs::create_dir_all(dir.join("source").join("world")).unwrap();
        fs::write(
            dir.join("source").join("server.properties"),
            "level-name=world\n",
        )
        .unwrap();
        fs::write(dir.join("source").join("world").join("level.dat"), "world").unwrap();

        let mut store = InstanceStore::open(root.path().join("instances.db"), &dir).unwrap();
        let source = dir.join("source");
        let duplicate = store.duplicate_instance(&source, "copy").unwrap();

        assert!(duplicate.path.exists());
        assert!(duplicate.path.join("server.properties").exists());
        assert!(!duplicate.path.join("world").exists());
    }

    #[test]
    fn duplicate_named_instance_preserves_version_and_loader_and_deletes_cleanly() {
        let root = tempdir().unwrap();
        let dir = root.path().join("servers");
        let mut store = InstanceStore::open(root.path().join("instances.db"), &dir).unwrap();

        let source = store
            .create_instance(
                "neoforge_srv",
                "1.21.4",
                "neoforge",
                dir.join("neoforge_srv"),
            )
            .unwrap();
        fs::write(source.path.join("server.properties"), "motd=test\n").unwrap();
        fs::write(source.world.join("level.dat"), "world-bytes").unwrap();

        let dup = store
            .duplicate_named_instance("neoforge_srv", "neoforge_srv_copy")
            .unwrap();
        assert_eq!(dup.name, "neoforge_srv_copy");
        assert_eq!(dup.minecraft_version, "1.21.4");
        assert_eq!(dup.loader, "neoforge");
        assert!(dup.path.join("server.properties").exists());
        assert!(!dup.path.join("world").exists());

        store.delete_instance("neoforge_srv_copy").unwrap();
        assert!(!dup.path.exists());
        assert_eq!(store.list_instances().unwrap().len(), 1);
    }
}
