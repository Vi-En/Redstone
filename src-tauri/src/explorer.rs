use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ExplorerError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("path escapes the instance directory: {0}")]
    Escape(String),
    #[error("delete requires confirmation: {0}")]
    DeleteRequiresConfirmation(String),
    #[error("file not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub relative_path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Clone)]
pub struct FileExplorer {
    root: PathBuf,
}

impl FileExplorer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn safe_path(&self, relative_path: &str) -> Result<PathBuf, ExplorerError> {
        let path = Path::new(relative_path);
        if path.is_absolute() {
            return Err(ExplorerError::Escape(relative_path.to_string()));
        }

        if path
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(ExplorerError::Escape(relative_path.to_string()));
        }

        Ok(self.root.join(path))
    }

    pub fn list_directory(&self, relative_path: &str) -> Result<Vec<FileEntry>, ExplorerError> {
        let dir_path = self.safe_path(relative_path)?;
        if !dir_path.exists() {
            return Err(ExplorerError::NotFound(relative_path.to_string()));
        }
        let read_dir = fs::read_dir(&dir_path)?;
        let mut entries = Vec::new();
        for entry in read_dir {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            let is_dir = metadata.is_dir();
            let size_bytes = if is_dir { 0 } else { metadata.len() };
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let rel_path = if relative_path.trim().is_empty() || relative_path == "." {
                file_name.clone()
            } else {
                format!("{}/{}", relative_path.trim_end_matches('/'), file_name)
            };

            entries.push(FileEntry {
                name: file_name,
                relative_path: rel_path,
                is_dir,
                size_bytes,
                modified_ms,
            });
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub fn read_file(&self, relative_path: &str) -> Result<String, ExplorerError> {
        let path = self.safe_path(relative_path)?;
        if !path.exists() {
            return Err(ExplorerError::NotFound(relative_path.to_string()));
        }
        Ok(fs::read_to_string(path)?)
    }

    pub fn save_file(&self, relative_path: &str, contents: &str) -> Result<(), ExplorerError> {
        let path = self.safe_path(relative_path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn write_file(&self, relative_path: &str, contents: &str) -> Result<(), ExplorerError> {
        self.save_file(relative_path, contents)
    }

    pub fn delete_file(&self, relative_path: &str, confirmed: bool) -> Result<(), ExplorerError> {
        if !confirmed {
            return Err(ExplorerError::DeleteRequiresConfirmation(
                relative_path.to_string(),
            ));
        }

        let path = self.safe_path(relative_path)?;
        if !path.exists() {
            return Err(ExplorerError::NotFound(relative_path.to_string()));
        }

        fs::remove_file(path)?;
        Ok(())
    }

    pub fn upload_jar(
        &self,
        source: impl AsRef<Path>,
        target_dir: &str,
    ) -> Result<PathBuf, ExplorerError> {
        let source = source.as_ref();
        if !source.exists() {
            return Err(ExplorerError::NotFound(source.display().to_string()));
        }

        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ExplorerError::NotFound(source.display().to_string()))?;

        let target_dir = self.safe_path(target_dir)?;
        fs::create_dir_all(&target_dir)?;

        let destination = target_dir.join(file_name);
        fs::copy(source, &destination)?;
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn editing_and_saving_toml_does_not_corrupt_it() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(&instance_dir).unwrap();
        fs::write(instance_dir.join("config.toml"), "name = \"demo\"\n").unwrap();

        let explorer = FileExplorer::new(&instance_dir);
        explorer
            .save_file("config.toml", "name = \"demo\"\nversion = \"1.0\"\n")
            .unwrap();

        let contents = fs::read_to_string(instance_dir.join("config.toml")).unwrap();
        assert!(contents.contains("version = \"1.0\""));
    }

    #[test]
    fn uploaded_jar_dropped_on_mods_lands_in_mods() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(instance_dir.join("mods")).unwrap();

        let jar = root.path().join("uploaded.jar");
        fs::write(&jar, b"jar").unwrap();

        let explorer = FileExplorer::new(&instance_dir);
        let saved = explorer.upload_jar(&jar, "mods").unwrap();

        assert!(saved.starts_with(instance_dir.join("mods")));
        assert!(instance_dir.join("mods").join("uploaded.jar").exists());
    }

    #[test]
    fn delete_requires_confirmation() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(&instance_dir).unwrap();
        fs::write(instance_dir.join("notes.txt"), "hi").unwrap();

        let explorer = FileExplorer::new(&instance_dir);
        assert!(explorer.delete_file("notes.txt", false).is_err());
        assert!(instance_dir.join("notes.txt").exists());
    }

    #[test]
    fn paths_cannot_escape_the_instance_directory() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(&instance_dir).unwrap();

        let explorer = FileExplorer::new(&instance_dir);
        assert!(explorer.read_file("../outside.txt").is_err());
        assert!(explorer.write_file("../outside.txt", "bad").is_err());
    }

    #[test]
    fn list_directory_returns_sorted_entries_and_prevents_escape() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(instance_dir.join("mods")).unwrap();
        fs::write(instance_dir.join("server.properties"), "server-port=25565").unwrap();
        fs::write(instance_dir.join("mods").join("test.jar"), "mod").unwrap();

        let explorer = FileExplorer::new(&instance_dir);
        let entries = explorer.list_directory("").unwrap();
        assert_eq!(entries.len(), 2);
        // directories come first
        assert_eq!(entries[0].name, "mods");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].name, "server.properties");
        assert!(!entries[1].is_dir);

        let sub_entries = explorer.list_directory("mods").unwrap();
        assert_eq!(sub_entries.len(), 1);
        assert_eq!(sub_entries[0].name, "test.jar");
        assert_eq!(sub_entries[0].relative_path, "mods/test.jar");

        assert!(explorer.list_directory("../..").is_err());
    }
}
