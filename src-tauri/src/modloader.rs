use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderMetadata {
    pub name: String,
    pub compatible_versions: Vec<String>,
}

impl LoaderMetadata {
    pub fn new(name: impl Into<String>, compatible_versions: Vec<String>) -> Self {
        Self {
            name: name.into(),
            compatible_versions,
        }
    }
}

#[derive(Debug, Default)]
pub struct ModLoaderManager {
    cache: HashMap<(String, String), String>,
    network_calls: usize,
}

impl ModLoaderManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn metadata() -> Vec<LoaderMetadata> {
        vec![
            LoaderMetadata::new("fabric", vec!["1.20.1".to_string(), "1.21.1".to_string()]),
            LoaderMetadata::new("forge", vec!["1.20.1".to_string(), "1.20.4".to_string()]),
            LoaderMetadata::new("neoforge", vec!["1.20.6".to_string(), "1.21.1".to_string()]),
        ]
    }

    pub fn compatibility_matrix(&self) -> HashMap<String, Vec<String>> {
        Self::metadata()
            .into_iter()
            .map(|metadata| (metadata.name.clone(), metadata.compatible_versions))
            .collect()
    }

    pub fn resolve_version(&mut self, loader: &str, version: &str) -> Result<String, String> {
        let key = (loader.to_string(), version.to_string());
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }

        let metadata = Self::metadata();
        let supported = metadata
            .into_iter()
            .find(|entry| entry.name == loader)
            .ok_or_else(|| format!("unsupported loader: {loader}"))?;

        if !supported
            .compatible_versions
            .iter()
            .any(|candidate| candidate == version)
        {
            return Err(format!("{loader} does not support minecraft {version}"));
        }

        self.network_calls += 1;
        let resolved = format!("{loader}-{version}");
        self.cache.insert(key, resolved.clone());
        Ok(resolved)
    }

    pub fn network_calls(&self) -> usize {
        self.network_calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_matrix_is_built_from_loader_metadata() {
        let matrix = ModLoaderManager::new().compatibility_matrix();

        assert!(matrix.contains_key("fabric"));
        assert!(matrix["fabric"].contains(&"1.20.1".to_string()));
        assert!(matrix["neoforge"].contains(&"1.21.1".to_string()));
    }

    #[test]
    fn cached_version_reuse_does_not_trigger_a_second_network_call() {
        let mut manager = ModLoaderManager::new();
        let first = manager.resolve_version("fabric", "1.20.1").unwrap();
        let second = manager.resolve_version("fabric", "1.20.1").unwrap();

        assert_eq!(first, second);
        assert_eq!(manager.network_calls(), 1);
    }
}
