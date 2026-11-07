use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// User preferences and non-secret onboarding state persisted in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Whether the initial setup flow has been completed.
    pub onboarding_complete: bool,
    /// Restrict the server to local network access by default.
    pub lan_only: bool,
    /// The instance selected during initial setup.
    pub instance_name: Option<String>,
    /// DNS provider selected by the user; credentials belong in the OS keychain.
    pub dns_provider: Option<String>,
    /// DNS subdomain selected by the user.
    pub dns_subdomain: Option<String>,
    /// DNS domain selected by the user.
    pub dns_domain: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            lan_only: true,
            instance_name: None,
            dns_provider: None,
            dns_subdomain: None,
            dns_domain: None,
        }
    }
}

/// Errors encountered while loading or saving local settings.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The settings file could not be read or written.
    #[error("settings filesystem error: {0}")]
    Io(#[from] io::Error),
    /// The settings file contains invalid TOML.
    #[error("settings TOML is invalid: {0}")]
    Toml(#[from] toml::de::Error),
    /// Settings could not be serialized as TOML.
    #[error("settings TOML serialization failed: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    /// A DNS hostname contains an invalid label.
    #[error("invalid DNS hostname: {0}")]
    InvalidDnsName(String),
}

/// Reads and writes settings at a user-selected local path.
#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// Creates a settings store for the given TOML file path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Loads settings, returning safe defaults when the file does not exist.
    pub fn load(&self) -> Result<Settings, SettingsError> {
        if !self.path.exists() {
            return Ok(Settings::default());
        }

        let contents = fs::read_to_string(&self.path)?;
        Ok(toml::from_str(&contents)?)
    }

    /// Writes settings as human-editable TOML, creating parent directories.
    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        validate_dns_name(settings.dns_subdomain.as_deref(), true)?;
        validate_dns_name(settings.dns_domain.as_deref(), false)?;

        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }

        fs::write(&self.path, toml::to_string_pretty(settings)?)?;
        Ok(())
    }
}

fn validate_dns_name(name: Option<&str>, single_label: bool) -> Result<(), SettingsError> {
    let Some(name) = name else {
        return Ok(());
    };

    let labels: Vec<&str> = name.split('.').collect();
    if name.is_empty()
        || (single_label && labels.len() != 1)
        || name.len() > 253
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(SettingsError::InvalidDnsName(name.to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_settings_file_uses_safe_first_run_defaults() {
        let root = tempdir().unwrap();
        let settings = SettingsStore::new(root.path().join("settings.toml"))
            .load()
            .unwrap();

        assert!(!settings.onboarding_complete);
        assert!(settings.lan_only);
        assert_eq!(settings.instance_name, None);
    }

    #[test]
    fn settings_round_trip_without_persisting_credentials() {
        let root = tempdir().unwrap();
        let path = root.path().join("settings.toml");
        let store = SettingsStore::new(&path);
        let settings = Settings {
            onboarding_complete: true,
            lan_only: false,
            instance_name: Some("survival".to_string()),
            dns_provider: Some("cloudflare".to_string()),
            dns_subdomain: Some("play".to_string()),
            dns_domain: Some("example.com".to_string()),
        };

        store.save(&settings).unwrap();

        assert_eq!(store.load().unwrap(), settings);
        let contents = fs::read_to_string(path).unwrap();
        assert!(!contents.contains("token"));
    }

    #[test]
    fn malformed_settings_are_reported_instead_of_silently_reset() {
        let root = tempdir().unwrap();
        let path = root.path().join("settings.toml");
        fs::write(&path, "lan_only = [\n").unwrap();

        let error = SettingsStore::new(path).load().unwrap_err();

        assert!(matches!(error, SettingsError::Toml(_)));
    }

    #[test]
    fn dns_settings_reject_invalid_host_labels() {
        let root = tempdir().unwrap();
        let store = SettingsStore::new(root.path().join("settings.toml"));
        let settings = Settings {
            dns_subdomain: Some("../outside".to_string()),
            ..Settings::default()
        };

        assert!(matches!(
            store.save(&settings),
            Err(SettingsError::InvalidDnsName(_))
        ));
    }
}
