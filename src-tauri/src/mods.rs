use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
/// Errors returned by mod catalog and local mod-file operations.
pub enum ModError {
    #[error("mod file system error: {0}")]
    Io(#[from] io::Error),
    #[error("mod not found: {0}")]
    NotFound(String),
    #[error("invalid mod file: {0}")]
    InvalidMod(String),
    #[error("invalid mod search: {0}")]
    InvalidSearch(String),
    #[error("mod catalog request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("could not encode mod search filters: {0}")]
    SearchEncoding(#[from] serde_json::Error),
    #[error("invalid Modrinth project ID")]
    InvalidProjectId,
    #[error("no compatible JAR was found for this mod")]
    NoCompatibleFile,
    #[error("Modrinth returned an unsafe download URL")]
    UnsafeDownloadUrl,
    #[error("mod download exceeds the 128 MiB limit")]
    DownloadTooLarge,
    #[error("mod is already installed: {0}")]
    AlreadyInstalled(String),
    #[error("mod installation requires the async Modrinth installer")]
    AsyncInstallRequired,
    #[error("mod filename is invalid: {0}")]
    InvalidModName(String),
    #[error("removing a mod requires confirmation")]
    ConfirmationRequired,
    #[error("the installed mod version changed; refresh the update list")]
    StaleUpdate,
    #[error("no update is available for this mod")]
    NoUpdateAvailable,
    #[error("CurseForge API key is missing. Set it in settings or the mod catalog")]
    CurseForgeApiKeyMissing,
    #[error("CurseForge rate limit exceeded. Please try again later")]
    CurseForgeRateLimited,
    #[error("mod distribution is disabled by the author: {project_url}")]
    DistributionDisabled { project_url: String },
    #[error("CurseForge request failed after 3 attempts: {0}")]
    RequestRetriesExhausted(String),
    #[error("CurseForge API error ({status}): {message}")]
    CurseForgeApi { status: u16, message: String },
    #[error("invalid CurseForge response schema: {0}")]
    InvalidSchema(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// A mod project returned by the Modrinth or CurseForge catalog.
pub struct ModSearchResult {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_url: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub distribution_disabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// A Modrinth project and version associated with an installed JAR.
pub struct TrackedMod {
    pub project_id: String,
    pub file_name: String,
    pub version_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// A newer compatible Modrinth version available for an installed mod.
pub struct ModUpdate {
    pub project_id: String,
    pub file_name: String,
    pub current_version_id: String,
    pub latest_version_id: String,
    pub latest_version_number: String,
    pub latest_file_name: String,
}

#[derive(Clone)]
/// HTTP client for searching compatible mods on Modrinth.
pub struct ModrinthClient {
    client: reqwest::Client,
    base_url: String,
}

impl ModrinthClient {
    /// Creates a client configured for the public Modrinth API.
    pub fn new() -> Self {
        Self::with_base_url("https://api.modrinth.com")
    }

    fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Searches mod projects for the requested Minecraft version and loader.
    pub async fn search(
        &self,
        query: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Vec<ModSearchResult>, ModError> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 100 {
            return Err(ModError::InvalidSearch(
                "query must contain 1-100 characters".to_string(),
            ));
        }
        if minecraft_version.is_empty()
            || minecraft_version.len() > 32
            || !minecraft_version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
        {
            return Err(ModError::InvalidSearch(
                "invalid Minecraft version".to_string(),
            ));
        }
        if !["vanilla", "fabric", "forge", "neoforge"].contains(&loader) {
            return Err(ModError::InvalidSearch(
                "unsupported mod loader".to_string(),
            ));
        }

        let mut facets = vec![
            vec!["project_type:mod".to_string()],
            vec![format!("versions:{minecraft_version}")],
        ];
        if loader != "vanilla" {
            facets.push(vec![format!("categories:{loader}")]);
        }
        let facets = serde_json::to_string(&facets)?;
        let url = format!("{}/v2/search", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(20))
            .query(&[("query", query), ("facets", &facets), ("limit", "20")])
            .send()
            .await?
            .error_for_status()?
            .json::<ModrinthSearchResponse>()
            .await?;

        Ok(response.hits)
    }

    /// Checks one tracked mod for a newer compatible version without downloading it.
    pub async fn check_mod_update(
        &self,
        installed: &TrackedMod,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Option<ModUpdate>, ModError> {
        Self::validate_tracked_mod(installed)?;
        let mut versions = self
            .compatible_versions(&installed.project_id, minecraft_version, loader)
            .await?;
        versions.sort_by(|left, right| right.date_published.cmp(&left.date_published));
        let latest = versions.first().ok_or(ModError::NoCompatibleFile)?;
        if latest.id == installed.version_id {
            return Ok(None);
        }
        let file = latest
            .files
            .iter()
            .find(|file| file.primary && is_jar_filename(&file.filename))
            .or_else(|| {
                latest
                    .files
                    .iter()
                    .find(|file| is_jar_filename(&file.filename))
            })
            .ok_or(ModError::NoCompatibleFile)?;

        Ok(Some(ModUpdate {
            project_id: installed.project_id.clone(),
            file_name: installed.file_name.clone(),
            current_version_id: installed.version_id.clone(),
            latest_version_id: latest.id.clone(),
            latest_version_number: latest.version_number.clone(),
            latest_file_name: if installed.file_name.ends_with(".disabled") {
                format!("{}.disabled", file.filename)
            } else {
                file.filename.clone()
            },
        }))
    }

    /// Replaces a tracked mod with its latest compatible version after confirmation.
    pub async fn apply_mod_update(
        &self,
        instance_dir: impl AsRef<Path>,
        update: &ModUpdate,
        minecraft_version: &str,
        loader: &str,
        confirmed: bool,
    ) -> Result<TrackedMod, ModError> {
        if !confirmed {
            return Err(ModError::ConfirmationRequired);
        }
        let installed = TrackedMod {
            project_id: update.project_id.clone(),
            file_name: update.file_name.clone(),
            version_id: update.current_version_id.clone(),
        };
        Self::validate_tracked_mod(&installed)?;
        let current_path = instance_dir.as_ref().join("mods").join(&update.file_name);
        let metadata = tokio::fs::symlink_metadata(&current_path).await?;
        if !metadata.file_type().is_file() {
            return Err(ModError::InvalidModName(update.file_name.clone()));
        }

        let current_update = self
            .check_mod_update(&installed, minecraft_version, loader)
            .await?
            .ok_or(ModError::NoUpdateAvailable)?;
        if current_update.latest_version_id != update.latest_version_id
            || current_update.latest_file_name != update.latest_file_name
        {
            return Err(ModError::StaleUpdate);
        }

        let versions = self
            .compatible_versions(&installed.project_id, minecraft_version, loader)
            .await?;
        let version = versions
            .iter()
            .find(|version| version.id == update.latest_version_id)
            .ok_or(ModError::StaleUpdate)?;
        let file = version
            .files
            .iter()
            .find(|file| {
                file.filename
                    == update
                        .latest_file_name
                        .strip_suffix(".disabled")
                        .unwrap_or(&update.latest_file_name)
            })
            .ok_or(ModError::StaleUpdate)?;
        let download_url = file
            .url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        let api_url = self
            .base_url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        if !is_safe_modrinth_download_url(&download_url, &api_url)
            || !is_jar_filename(
                update
                    .latest_file_name
                    .strip_suffix(".disabled")
                    .unwrap_or(&update.latest_file_name),
            )
        {
            return Err(ModError::UnsafeDownloadUrl);
        }

        let download_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = download_client
            .get(download_url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(ModError::UnsafeDownloadUrl);
        }
        let response = response.error_for_status()?;
        if !is_safe_modrinth_download_url(response.url(), &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }
        let bytes = Self::read_bounded_response(response).await?;

        let mods_dir = instance_dir.as_ref().join("mods");
        let target_path = mods_dir.join(&update.latest_file_name);
        if target_path != current_path && tokio::fs::try_exists(&target_path).await? {
            return Err(ModError::AlreadyInstalled(update.latest_file_name.clone()));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = mods_dir.join(format!(".redstone-update-{nonce}.tmp"));
        let mut temp_file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await?;
        if let Err(error) = temp_file.write_all(&bytes).await {
            drop(temp_file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ModError::Io(error));
        }
        if let Err(error) = temp_file.flush().await {
            drop(temp_file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ModError::Io(error));
        }
        drop(temp_file);

        if target_path == current_path {
            let backup_path = mods_dir.join(format!(".redstone-backup-{nonce}.tmp"));
            tokio::fs::rename(&current_path, &backup_path).await?;
            if let Err(error) = tokio::fs::rename(&temp_path, &target_path).await {
                let _ = tokio::fs::rename(&backup_path, &current_path).await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(ModError::Io(error));
            }
            tokio::fs::remove_file(backup_path).await?;
        } else {
            tokio::fs::rename(&temp_path, &target_path).await?;
            tokio::fs::remove_file(current_path).await?;
        }

        let updated = TrackedMod {
            project_id: update.project_id.clone(),
            file_name: update.latest_file_name.clone(),
            version_id: update.latest_version_id.clone(),
        };
        let record = updated.clone();
        let instance_dir = instance_dir.as_ref().to_path_buf();
        tokio::task::spawn_blocking(move || {
            ModManager::new().record_tracked_mod(instance_dir, record)
        })
        .await
        .map_err(|error| ModError::Io(io::Error::other(error.to_string())))??;
        Ok(updated)
    }

    async fn compatible_versions(
        &self,
        project_id: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Vec<ModrinthVersion>, ModError> {
        if project_id.is_empty()
            || project_id.len() > 64
            || !project_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ModError::InvalidProjectId);
        }
        Ok(self
            .client
            .get(format!(
                "{}/v2/project/{project_id}/version",
                self.base_url.trim_end_matches('/')
            ))
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(20))
            .query(&[
                (
                    "game_versions",
                    serde_json::to_string(&[minecraft_version])?,
                ),
                ("loaders", serde_json::to_string(&[loader])?),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<ModrinthVersion>>()
            .await?)
    }

    async fn read_bounded_response(response: reqwest::Response) -> Result<Vec<u8>, ModError> {
        const MAX_DOWNLOAD_BYTES: usize = 128 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOWNLOAD_BYTES as u64)
        {
            return Err(ModError::DownloadTooLarge);
        }
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
                return Err(ModError::DownloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    fn validate_tracked_mod(installed: &TrackedMod) -> Result<(), ModError> {
        if installed.project_id.is_empty()
            || installed.project_id.len() > 64
            || !installed
                .project_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || installed.version_id.is_empty()
            || installed.version_id.len() > 64
            || !installed
                .version_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || !is_mod_file_name(&installed.file_name)
        {
            return Err(ModError::InvalidProjectId);
        }
        Ok(())
    }
    /// Downloads a compatible Modrinth mod JAR into an instance's `mods/` folder.
    pub async fn install_compatible_mod(
        &self,
        instance_dir: impl AsRef<Path>,
        project_id: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<PathBuf, ModError> {
        if project_id.is_empty()
            || project_id.len() > 64
            || !project_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ModError::InvalidProjectId);
        }

        let mut query = self
            .client
            .get(format!(
                "{}/v2/project/{project_id}/version",
                self.base_url.trim_end_matches('/')
            ))
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(20))
            .query(&[
                (
                    "game_versions",
                    serde_json::to_string(&[minecraft_version])?,
                ),
                ("loaders", serde_json::to_string(&[loader])?),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<ModrinthVersion>>()
            .await?;

        query.sort_by(|left, right| right.date_published.cmp(&left.date_published));
        let version = query
            .iter()
            .find(|version| {
                version
                    .files
                    .iter()
                    .any(|file| file.primary && is_jar_filename(&file.filename))
            })
            .or_else(|| {
                query.iter().find(|version| {
                    version
                        .files
                        .iter()
                        .any(|file| is_jar_filename(&file.filename))
                })
            })
            .ok_or(ModError::NoCompatibleFile)?;
        let file = version
            .files
            .iter()
            .find(|file| file.primary && is_jar_filename(&file.filename))
            .or_else(|| {
                version
                    .files
                    .iter()
                    .find(|file| is_jar_filename(&file.filename))
            })
            .ok_or(ModError::NoCompatibleFile)?;
        let version_id = version.id.clone();

        let download_url = file
            .url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        let api_url = self
            .base_url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        if !is_safe_modrinth_download_url(&download_url, &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }

        if !is_jar_filename(&file.filename) {
            return Err(ModError::NoCompatibleFile);
        }
        let mods_dir = instance_dir.as_ref().join("mods");
        tokio::fs::create_dir_all(&mods_dir).await?;
        let installed_path = mods_dir.join(&file.filename);
        if tokio::fs::try_exists(&installed_path).await? {
            return Err(ModError::AlreadyInstalled(file.filename.clone()));
        }

        let download_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = download_client
            .get(download_url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(ModError::UnsafeDownloadUrl);
        }
        let response = response.error_for_status()?;
        if !is_safe_modrinth_download_url(response.url(), &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }
        const MAX_DOWNLOAD_BYTES: usize = 128 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOWNLOAD_BYTES as u64)
        {
            return Err(ModError::DownloadTooLarge);
        }

        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
                return Err(ModError::DownloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }

        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&installed_path)
            .await
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    ModError::AlreadyInstalled(file.filename.clone())
                } else {
                    ModError::Io(error)
                }
            })?;
        if let Err(error) = output.write_all(&bytes).await {
            drop(output);
            let _ = tokio::fs::remove_file(&installed_path).await;
            return Err(ModError::Io(error));
        }
        if let Err(error) = output.flush().await {
            drop(output);
            let _ = tokio::fs::remove_file(&installed_path).await;
            return Err(ModError::Io(error));
        }

        let instance_dir = instance_dir.as_ref().to_path_buf();
        let tracked = TrackedMod {
            project_id: project_id.to_string(),
            file_name: file.filename.clone(),
            version_id,
        };
        tokio::task::spawn_blocking(move || {
            ModManager::new().record_tracked_mod(instance_dir, tracked)
        })
        .await
        .map_err(|error| ModError::Io(io::Error::other(error.to_string())))??;

        Ok(installed_path)
    }
}

fn is_safe_modrinth_download_url(url: &reqwest::Url, api_url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    let api_host = api_url.host_str().unwrap_or_default();
    let is_modrinth_host = host == "modrinth.com" || host.ends_with(".modrinth.com");
    let is_test_host = api_url.scheme() == "http" && host == api_host;
    (url.scheme() == "https" && is_modrinth_host) || is_test_host
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModSearchResult>,
}

#[derive(Deserialize)]
struct ModrinthVersion {
    #[serde(default)]
    id: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    date_published: String,
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
}

fn is_jar_filename(filename: &str) -> bool {
    filename.ends_with(".jar")
        && filename.len() <= 255
        && filename
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// The default CurseForge API rate limit per minute per API key.
pub const CURSEFORGE_RATE_LIMIT_PER_MINUTE: u32 = 120;

#[derive(Debug)]
struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_rate_per_sec: f64,
    last_refill: std::time::Instant,
}

impl TokenBucket {
    fn new(per_minute: u32) -> Self {
        let capacity = per_minute as f64;
        let refill_rate_per_sec = capacity / 60.0;
        Self {
            capacity,
            tokens: capacity,
            refill_rate_per_sec,
            last_refill: std::time::Instant::now(),
        }
    }

    fn try_acquire(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.refill_rate_per_sec).min(self.capacity);
    }

    fn update_from_headers(&mut self, remaining: Option<u32>, limit: Option<u32>) {
        if let Some(limit_val) = limit {
            if limit_val > 0 && limit_val != self.capacity as u32 {
                self.capacity = limit_val as f64;
                self.refill_rate_per_sec = self.capacity / 60.0;
            }
        }
        if let Some(rem) = remaining {
            self.tokens = (rem as f64).min(self.capacity);
        }
    }
}

#[derive(Clone, Debug)]
struct SearchCacheEntry {
    timestamp: std::time::Instant,
    results: Vec<ModSearchResult>,
}

#[derive(Clone)]
/// HTTP client for searching, downloading, and updating mods on CurseForge.
pub struct CurseForgeClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    rate_limiter: Arc<Mutex<TokenBucket>>,
    search_cache: Arc<Mutex<HashMap<String, SearchCacheEntry>>>,
}

impl CurseForgeClient {
    /// Creates a client configured with the official CurseForge API and an optional API key.
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url("https://api.curseforge.com", api_key)
    }

    /// Creates a client with a custom base URL (useful for testing with mock servers).
    pub fn with_base_url(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            rate_limiter: Arc::new(Mutex::new(TokenBucket::new(
                CURSEFORGE_RATE_LIMIT_PER_MINUTE,
            ))),
            search_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn api_key(&self) -> Result<&str, ModError> {
        self.api_key
            .as_deref()
            .ok_or(ModError::CurseForgeApiKeyMissing)
    }

    async fn execute_with_retries(
        &self,
        make_request: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ModError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            {
                let mut bucket = self.rate_limiter.lock().await;
                if !bucket.try_acquire() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }

            let request = make_request();
            let result = request.send().await;

            match result {
                Ok(response) => {
                    let remaining = response
                        .headers()
                        .get("x-ratelimit-remaining")
                        .and_then(|v| v.to_str().ok()?.parse().ok());
                    let limit = response
                        .headers()
                        .get("x-ratelimit-limit")
                        .and_then(|v| v.to_str().ok()?.parse().ok());
                    {
                        let mut bucket = self.rate_limiter.lock().await;
                        bucket.update_from_headers(remaining, limit);
                    }

                    let status = response.status();
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        if attempt >= 3 {
                            return Err(ModError::CurseForgeRateLimited);
                        }
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
                            .map(Duration::from_secs)
                            .unwrap_or_else(|| Duration::from_millis(50 * (1 << attempt)));
                        tokio::time::sleep(retry_after).await;
                        continue;
                    }

                    if status.is_server_error() {
                        if attempt >= 3 {
                            return Err(ModError::RequestRetriesExhausted(format!(
                                "CurseForge returned HTTP {status}"
                            )));
                        }
                        tokio::time::sleep(Duration::from_millis(50 * (1 << attempt))).await;
                        continue;
                    }

                    if !status.is_success() {
                        let text = response.text().await.unwrap_or_default();
                        return Err(ModError::CurseForgeApi {
                            status: status.as_u16(),
                            message: text,
                        });
                    }

                    return Ok(response);
                }
                Err(err) if (err.is_timeout() || err.is_connect()) && attempt < 3 => {
                    tokio::time::sleep(Duration::from_millis(50 * (1 << attempt))).await;
                    continue;
                }
                Err(err) => {
                    if attempt >= 3 {
                        return Err(ModError::RequestRetriesExhausted(err.to_string()));
                    } else {
                        return Err(ModError::Request(err));
                    }
                }
            }
        }
    }

    /// Searches CurseForge for mods matching the query, Minecraft version, and loader.
    pub async fn search(
        &self,
        query: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Vec<ModSearchResult>, ModError> {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 100 {
            return Err(ModError::InvalidSearch(
                "query must contain 1-100 characters".to_string(),
            ));
        }
        if minecraft_version.is_empty()
            || minecraft_version.len() > 32
            || !minecraft_version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
        {
            return Err(ModError::InvalidSearch(
                "invalid Minecraft version".to_string(),
            ));
        }
        if !["vanilla", "fabric", "forge", "neoforge"].contains(&loader) {
            return Err(ModError::InvalidSearch(
                "unsupported mod loader".to_string(),
            ));
        }

        let api_key = self.api_key()?;
        let cache_key = format!("{query}:{minecraft_version}:{loader}");

        // Check 5-minute cache
        {
            let cache = self.search_cache.lock().await;
            if let Some(entry) = cache.get(&cache_key) {
                if entry.timestamp.elapsed() < Duration::from_secs(300) {
                    return Ok(entry.results.clone());
                }
            }
        }

        let url = format!("{}/v1/mods/search", self.base_url.trim_end_matches('/'));
        let mut query_params: Vec<(&str, String)> = vec![
            ("gameId", "432".to_string()),
            ("classId", "6".to_string()),
            ("searchFilter", query.to_string()),
            ("gameVersion", minecraft_version.to_string()),
            ("pageSize", "20".to_string()),
        ];
        if let Some(loader_type) = curseforge_mod_loader_type(loader) {
            query_params.push(("modLoaderType", loader_type.to_string()));
        }

        let response = self
            .execute_with_retries(|| {
                self.client
                    .get(&url)
                    .header(
                        reqwest::header::USER_AGENT,
                        concat!("Redstone/", env!("CARGO_PKG_VERSION")),
                    )
                    .header("x-api-key", api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
                    .query(&query_params)
            })
            .await?;

        let body = response.text().await?;
        let search_response: CurseForgeSearchResponse =
            serde_json::from_str(&body).map_err(|e| {
                ModError::InvalidSchema(format!("Failed to parse search response: {e}"))
            })?;

        let results: Vec<ModSearchResult> = search_response
            .data
            .into_iter()
            .map(|item| {
                let project_url = item
                    .links
                    .as_ref()
                    .and_then(|l| l.website_url.clone())
                    .unwrap_or_else(|| {
                        format!("https://www.curseforge.com/minecraft/mc-mods/{}", item.slug)
                    });
                let distribution_disabled = item.allow_mod_distribution == Some(false);
                let author = item
                    .authors
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let icon_url = item.logo.and_then(|l| l.thumbnail_url.or(l.url));

                ModSearchResult {
                    project_id: format!("curseforge-{}", item.id),
                    slug: item.slug,
                    title: item.name,
                    description: item.summary,
                    author,
                    downloads: item.download_count as u64,
                    icon_url,
                    source: Some("curseforge".to_string()),
                    project_url: Some(project_url),
                    distribution_disabled,
                    categories: item.categories.into_iter().map(|c| c.name).collect(),
                }
            })
            .collect();

        // Store into 5-minute cache
        {
            let mut cache = self.search_cache.lock().await;
            cache.insert(
                cache_key,
                SearchCacheEntry {
                    timestamp: std::time::Instant::now(),
                    results: results.clone(),
                },
            );
        }

        Ok(results)
    }

    /// Downloads a compatible CurseForge mod JAR into an instance's `mods/` directory.
    pub async fn install_compatible_mod(
        &self,
        instance_dir: impl AsRef<Path>,
        project_id: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<PathBuf, ModError> {
        let mod_id = parse_curseforge_mod_id(project_id)?;
        let api_key = self.api_key()?;

        // 1. Check mod distribution settings
        let mod_url = format!("{}/v1/mods/{mod_id}", self.base_url.trim_end_matches('/'));
        let mod_res = self
            .execute_with_retries(|| {
                self.client
                    .get(&mod_url)
                    .header(
                        reqwest::header::USER_AGENT,
                        concat!("Redstone/", env!("CARGO_PKG_VERSION")),
                    )
                    .header("x-api-key", api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
            })
            .await?;

        let mod_body = mod_res.text().await?;
        let mod_info: CurseForgeModResponse = serde_json::from_str(&mod_body)
            .map_err(|e| ModError::InvalidSchema(format!("Failed to parse mod details: {e}")))?;

        let project_url = mod_info
            .data
            .links
            .as_ref()
            .and_then(|l| l.website_url.clone())
            .unwrap_or_else(|| {
                format!(
                    "https://www.curseforge.com/minecraft/mc-mods/{}",
                    mod_info.data.slug
                )
            });

        if mod_info.data.allow_mod_distribution == Some(false) {
            return Err(ModError::DistributionDisabled { project_url });
        }

        // 2. Query compatible files
        let files_url = format!(
            "{}/v1/mods/{mod_id}/files",
            self.base_url.trim_end_matches('/')
        );
        let mut query_params: Vec<(&str, String)> = vec![
            ("gameVersion", minecraft_version.to_string()),
            ("pageSize", "20".to_string()),
        ];
        if let Some(loader_type) = curseforge_mod_loader_type(loader) {
            query_params.push(("modLoaderType", loader_type.to_string()));
        }

        let files_res = self
            .execute_with_retries(|| {
                self.client
                    .get(&files_url)
                    .header(
                        reqwest::header::USER_AGENT,
                        concat!("Redstone/", env!("CARGO_PKG_VERSION")),
                    )
                    .header("x-api-key", api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
                    .query(&query_params)
            })
            .await?;

        let files_body = files_res.text().await?;
        let files_info: CurseForgeFilesResponse = serde_json::from_str(&files_body)
            .map_err(|e| ModError::InvalidSchema(format!("Failed to parse files list: {e}")))?;

        let mut candidates: Vec<CurseForgeFileItem> = files_info
            .data
            .into_iter()
            .filter(|f| f.is_available && is_jar_filename(&f.file_name))
            .collect();

        if loader != "vanilla" {
            let loader_lower = loader.to_lowercase();
            let loader_candidates: Vec<CurseForgeFileItem> = candidates
                .iter()
                .filter(|f| {
                    f.game_versions
                        .iter()
                        .any(|v| v.to_lowercase() == loader_lower)
                })
                .cloned()
                .collect();
            if !loader_candidates.is_empty() {
                candidates = loader_candidates;
            }
        }

        // Sort by file id descending (highest fileId is newest)
        candidates.sort_by_key(|left| std::cmp::Reverse(left.id));
        let chosen_file = candidates.first().ok_or(ModError::NoCompatibleFile)?;

        let download_url_str = match &chosen_file.download_url {
            Some(url) if !url.trim().is_empty() => url,
            _ => return Err(ModError::DistributionDisabled { project_url }),
        };

        let download_url = download_url_str
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        let api_url = self
            .base_url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;

        if !is_safe_curseforge_download_url(&download_url, &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }

        let mods_dir = instance_dir.as_ref().join("mods");
        tokio::fs::create_dir_all(&mods_dir).await?;
        let installed_path = mods_dir.join(&chosen_file.file_name);
        if tokio::fs::try_exists(&installed_path).await? {
            return Err(ModError::AlreadyInstalled(chosen_file.file_name.clone()));
        }

        let download_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = download_client
            .get(download_url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(ModError::UnsafeDownloadUrl);
        }
        let response = response.error_for_status()?;
        if !is_safe_curseforge_download_url(response.url(), &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }

        const MAX_DOWNLOAD_BYTES: usize = 128 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|len| len > MAX_DOWNLOAD_BYTES as u64)
        {
            return Err(ModError::DownloadTooLarge);
        }

        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
                return Err(ModError::DownloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = mods_dir.join(format!(".redstone-install-{nonce}.tmp"));
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await?;
        if let Err(error) = output.write_all(&bytes).await {
            drop(output);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ModError::Io(error));
        }
        if let Err(error) = output.flush().await {
            drop(output);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ModError::Io(error));
        }
        drop(output);

        tokio::fs::rename(&temp_path, &installed_path).await?;

        let instance_dir = instance_dir.as_ref().to_path_buf();
        let tracked = TrackedMod {
            project_id: format!("curseforge-{mod_id}"),
            file_name: chosen_file.file_name.clone(),
            version_id: chosen_file.id.to_string(),
        };
        tokio::task::spawn_blocking(move || {
            ModManager::new().record_tracked_mod(instance_dir, tracked)
        })
        .await
        .map_err(|error| ModError::Io(io::Error::other(error.to_string())))??;

        Ok(installed_path)
    }

    /// Checks a tracked CurseForge mod for a newer compatible file.
    pub async fn check_mod_update(
        &self,
        installed: &TrackedMod,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Option<ModUpdate>, ModError> {
        let mod_id = parse_curseforge_mod_id(&installed.project_id)?;
        let api_key = self.api_key()?;
        let current_file_id: u32 = installed.version_id.parse().unwrap_or(0);

        let files_url = format!(
            "{}/v1/mods/{mod_id}/files",
            self.base_url.trim_end_matches('/')
        );
        let mut query_params: Vec<(&str, String)> = vec![
            ("gameVersion", minecraft_version.to_string()),
            ("pageSize", "20".to_string()),
        ];
        if let Some(loader_type) = curseforge_mod_loader_type(loader) {
            query_params.push(("modLoaderType", loader_type.to_string()));
        }

        let files_res = self
            .execute_with_retries(|| {
                self.client
                    .get(&files_url)
                    .header(
                        reqwest::header::USER_AGENT,
                        concat!("Redstone/", env!("CARGO_PKG_VERSION")),
                    )
                    .header("x-api-key", api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
                    .query(&query_params)
            })
            .await?;

        let files_body = files_res.text().await?;
        let files_info: CurseForgeFilesResponse = serde_json::from_str(&files_body)
            .map_err(|e| ModError::InvalidSchema(format!("Failed to parse files list: {e}")))?;

        let mut candidates: Vec<CurseForgeFileItem> = files_info
            .data
            .into_iter()
            .filter(|f| f.is_available && is_jar_filename(&f.file_name))
            .collect();

        if loader != "vanilla" {
            let loader_lower = loader.to_lowercase();
            let loader_candidates: Vec<CurseForgeFileItem> = candidates
                .iter()
                .filter(|f| {
                    f.game_versions
                        .iter()
                        .any(|v| v.to_lowercase() == loader_lower)
                })
                .cloned()
                .collect();
            if !loader_candidates.is_empty() {
                candidates = loader_candidates;
            }
        }

        candidates.sort_by_key(|left| std::cmp::Reverse(left.id));
        let latest = match candidates.first() {
            Some(f) => f,
            None => return Ok(None),
        };

        if latest.id <= current_file_id {
            return Ok(None);
        }

        let latest_file_name = if installed.file_name.ends_with(".disabled") {
            format!("{}.disabled", latest.file_name)
        } else {
            latest.file_name.clone()
        };

        Ok(Some(ModUpdate {
            project_id: installed.project_id.clone(),
            file_name: installed.file_name.clone(),
            current_version_id: installed.version_id.clone(),
            latest_version_id: latest.id.to_string(),
            latest_version_number: latest.display_name.clone(),
            latest_file_name,
        }))
    }

    /// Replaces a tracked CurseForge mod with its latest compatible version after confirmation.
    pub async fn apply_mod_update(
        &self,
        instance_dir: impl AsRef<Path>,
        update: &ModUpdate,
        minecraft_version: &str,
        loader: &str,
        confirmed: bool,
    ) -> Result<TrackedMod, ModError> {
        if !confirmed {
            return Err(ModError::ConfirmationRequired);
        }
        let installed = TrackedMod {
            project_id: update.project_id.clone(),
            file_name: update.file_name.clone(),
            version_id: update.current_version_id.clone(),
        };
        let current_path = instance_dir.as_ref().join("mods").join(&update.file_name);
        let metadata = tokio::fs::symlink_metadata(&current_path).await?;
        if !metadata.file_type().is_file() {
            return Err(ModError::InvalidModName(update.file_name.clone()));
        }

        let current_update = self
            .check_mod_update(&installed, minecraft_version, loader)
            .await?
            .ok_or(ModError::NoUpdateAvailable)?;
        if current_update.latest_version_id != update.latest_version_id
            || current_update.latest_file_name != update.latest_file_name
        {
            return Err(ModError::StaleUpdate);
        }

        let mod_id = parse_curseforge_mod_id(&update.project_id)?;
        let api_key = self.api_key()?;
        let files_url = format!(
            "{}/v1/mods/{mod_id}/files",
            self.base_url.trim_end_matches('/')
        );
        let mut query_params: Vec<(&str, String)> = vec![
            ("gameVersion", minecraft_version.to_string()),
            ("pageSize", "20".to_string()),
        ];
        if let Some(loader_type) = curseforge_mod_loader_type(loader) {
            query_params.push(("modLoaderType", loader_type.to_string()));
        }

        let files_res = self
            .execute_with_retries(|| {
                self.client
                    .get(&files_url)
                    .header(
                        reqwest::header::USER_AGENT,
                        concat!("Redstone/", env!("CARGO_PKG_VERSION")),
                    )
                    .header("x-api-key", api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .timeout(Duration::from_secs(20))
                    .query(&query_params)
            })
            .await?;

        let files_body = files_res.text().await?;
        let files_info: CurseForgeFilesResponse = serde_json::from_str(&files_body)
            .map_err(|e| ModError::InvalidSchema(format!("Failed to parse files list: {e}")))?;

        let target_file_id: u32 = update.latest_version_id.parse().unwrap_or(0);
        let chosen_file = files_info
            .data
            .into_iter()
            .find(|f| f.id == target_file_id)
            .ok_or(ModError::StaleUpdate)?;

        let download_url_str = chosen_file.download_url.ok_or(ModError::NoCompatibleFile)?;
        let download_url = download_url_str
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;
        let api_url = self
            .base_url
            .parse::<reqwest::Url>()
            .map_err(|_| ModError::UnsafeDownloadUrl)?;

        if !is_safe_curseforge_download_url(&download_url, &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }

        let download_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let response = download_client
            .get(download_url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("Redstone/", env!("CARGO_PKG_VERSION")),
            )
            .timeout(Duration::from_secs(60))
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(ModError::UnsafeDownloadUrl);
        }
        let response = response.error_for_status()?;
        if !is_safe_curseforge_download_url(response.url(), &api_url) {
            return Err(ModError::UnsafeDownloadUrl);
        }

        const MAX_DOWNLOAD_BYTES: usize = 128 * 1024 * 1024;
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DOWNLOAD_BYTES {
                return Err(ModError::DownloadTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }

        let mods_dir = instance_dir.as_ref().join("mods");
        let target_path = mods_dir.join(&update.latest_file_name);
        if target_path != current_path && tokio::fs::try_exists(&target_path).await? {
            return Err(ModError::AlreadyInstalled(update.latest_file_name.clone()));
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = mods_dir.join(format!(".redstone-update-{nonce}.tmp"));
        let mut temp_file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .await?;
        temp_file.write_all(&bytes).await?;
        temp_file.flush().await?;
        drop(temp_file);

        if target_path == current_path {
            let backup_path = mods_dir.join(format!(".redstone-backup-{nonce}.tmp"));
            tokio::fs::rename(&current_path, &backup_path).await?;
            if let Err(error) = tokio::fs::rename(&temp_path, &target_path).await {
                let _ = tokio::fs::rename(&backup_path, &current_path).await;
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(ModError::Io(error));
            }
            tokio::fs::remove_file(backup_path).await?;
        } else {
            tokio::fs::rename(&temp_path, &target_path).await?;
            tokio::fs::remove_file(current_path).await?;
        }

        let updated = TrackedMod {
            project_id: update.project_id.clone(),
            file_name: update.latest_file_name.clone(),
            version_id: update.latest_version_id.clone(),
        };
        let record = updated.clone();
        let instance_dir = instance_dir.as_ref().to_path_buf();
        tokio::task::spawn_blocking(move || {
            ModManager::new().record_tracked_mod(instance_dir, record)
        })
        .await
        .map_err(|error| ModError::Io(io::Error::other(error.to_string())))??;
        Ok(updated)
    }
}

fn is_safe_curseforge_download_url(url: &reqwest::Url, api_url: &reqwest::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    let api_host = api_url.host_str().unwrap_or_default();
    let is_curseforge_host = host == "forgecdn.net"
        || host.ends_with(".forgecdn.net")
        || host == "curseforge.com"
        || host.ends_with(".curseforge.com");
    let is_test_host = api_url.scheme() == "http" && host == api_host;
    (url.scheme() == "https" && is_curseforge_host) || is_test_host
}

fn parse_curseforge_mod_id(project_id: &str) -> Result<u32, ModError> {
    let raw = project_id
        .strip_prefix("curseforge-")
        .or_else(|| project_id.strip_prefix("cf-"))
        .unwrap_or(project_id);
    raw.parse::<u32>().map_err(|_| ModError::InvalidProjectId)
}

fn curseforge_mod_loader_type(loader: &str) -> Option<u32> {
    match loader {
        "forge" => Some(1),
        "fabric" => Some(4),
        "neoforge" => Some(6),
        _ => None,
    }
}

#[derive(Deserialize)]
struct CurseForgeSearchResponse {
    data: Vec<CurseForgeModItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeModResponse {
    data: CurseForgeModItem,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFilesResponse {
    data: Vec<CurseForgeFileItem>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeModItem {
    id: u32,
    name: String,
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: f64,
    #[serde(default)]
    authors: Vec<CurseForgeAuthorItem>,
    logo: Option<CurseForgeLogoItem>,
    links: Option<CurseForgeLinksItem>,
    #[serde(default = "default_some_true")]
    allow_mod_distribution: Option<bool>,
    #[serde(default)]
    categories: Vec<CurseForgeCategoryItem>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeCategoryItem {
    name: String,
}

fn default_some_true() -> Option<bool> {
    Some(true)
}

#[derive(Clone, Deserialize)]
struct CurseForgeAuthorItem {
    name: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeLogoItem {
    thumbnail_url: Option<String>,
    url: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeLinksItem {
    website_url: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct CurseForgeFileItem {
    id: u32,
    mod_id: u32,
    #[serde(default = "default_bool_true")]
    is_available: bool,
    display_name: String,
    file_name: String,
    release_type: Option<u32>,
    #[serde(default)]
    file_date: String,
    download_url: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
}

fn default_bool_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
/// A JAR file installed directly in an instance's `mods/` directory.
pub struct InstalledMod {
    pub file_name: String,
    pub enabled: bool,
}

#[derive(Debug, Default)]
pub struct ModManager;

impl ModManager {
    pub fn new() -> Self {
        Self
    }

    /// Rejects the former placeholder install path; use the async Modrinth installer.
    pub fn install_mod(
        &mut self,
        _instance_dir: impl AsRef<Path>,
        _file_name: &str,
        _source_url: &str,
    ) -> Result<PathBuf, ModError> {
        Err(ModError::AsyncInstallRequired)
    }

    /// Lists enabled and disabled JAR files in an instance's `mods/` directory.
    pub fn list_mods(&self, instance_dir: impl AsRef<Path>) -> Result<Vec<InstalledMod>, ModError> {
        let mods_dir = instance_dir.as_ref().join("mods");
        match fs::symlink_metadata(&mods_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(ModError::InvalidModName("mods".to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ModError::Io(error)),
        }

        let mut mods = Vec::new();
        for entry in fs::read_dir(&mods_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let file_name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            if file_name.ends_with(".jar") {
                mods.push(InstalledMod {
                    file_name,
                    enabled: true,
                });
            } else if file_name.ends_with(".jar.disabled") {
                mods.push(InstalledMod {
                    file_name,
                    enabled: false,
                });
            }
        }
        mods.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(mods)
    }

    /// Reads persisted Modrinth identity records that still point to installed JARs.
    pub fn tracked_mods(
        &self,
        instance_dir: impl AsRef<Path>,
    ) -> Result<Vec<TrackedMod>, ModError> {
        let instance_dir = instance_dir.as_ref();
        let manifest = instance_dir.join("mods").join(".redstone-tracked.json");
        let entries = match fs::read(&manifest) {
            Ok(contents) => serde_json::from_slice::<Vec<TrackedMod>>(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ModError::Io(error)),
        };
        let installed_files = self
            .list_mods(instance_dir)?
            .into_iter()
            .map(|mod_file| mod_file.file_name)
            .collect::<std::collections::HashSet<_>>();
        Ok(entries
            .into_iter()
            .filter(|entry| installed_files.contains(&entry.file_name))
            .collect())
    }

    /// Stores or replaces a tracked Modrinth identity in the instance-local manifest.
    pub fn record_tracked_mod(
        &self,
        instance_dir: impl AsRef<Path>,
        tracked: TrackedMod,
    ) -> Result<(), ModError> {
        ModrinthClient::validate_tracked_mod(&tracked)?;
        let mods_dir = instance_dir.as_ref().join("mods");
        fs::create_dir_all(&mods_dir)?;
        let manifest = mods_dir.join(".redstone-tracked.json");
        let mut entries = match fs::read(&manifest) {
            Ok(contents) => serde_json::from_slice::<Vec<TrackedMod>>(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(ModError::Io(error)),
        };
        entries.retain(|entry| {
            entry.project_id != tracked.project_id && entry.file_name != tracked.file_name
        });
        entries.push(tracked);
        write_tracked_manifest(&mods_dir, &manifest, &entries)
    }

    fn rename_tracked_mod(
        &self,
        instance_dir: &Path,
        old_file_name: &str,
        new_file_name: &str,
    ) -> Result<(), ModError> {
        let mods_dir = instance_dir.join("mods");
        let manifest = mods_dir.join(".redstone-tracked.json");
        let mut entries = match fs::read(&manifest) {
            Ok(contents) => serde_json::from_slice::<Vec<TrackedMod>>(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ModError::Io(error)),
        };
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.file_name == old_file_name)
        {
            entry.file_name = new_file_name.to_string();
            write_tracked_manifest(&mods_dir, &manifest, &entries)?;
        }
        Ok(())
    }

    fn forget_tracked_mod(&self, instance_dir: &Path, file_name: &str) -> Result<(), ModError> {
        let mods_dir = instance_dir.join("mods");
        let manifest = mods_dir.join(".redstone-tracked.json");
        let mut entries = match fs::read(&manifest) {
            Ok(contents) => serde_json::from_slice::<Vec<TrackedMod>>(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(ModError::Io(error)),
        };
        let original_len = entries.len();
        entries.retain(|entry| entry.file_name != file_name);
        if entries.len() != original_len {
            write_tracked_manifest(&mods_dir, &manifest, &entries)?;
        }
        Ok(())
    }

    /// Enables or disables a JAR by filename without leaving the instance's `mods/` directory.
    pub fn set_mod_enabled(
        &mut self,
        instance_dir: impl AsRef<Path>,
        file_name: &str,
        enabled: bool,
    ) -> Result<PathBuf, ModError> {
        let current_path = self.resolve_mod_path(instance_dir.as_ref(), file_name)?;
        let target_path = if enabled {
            match file_name.strip_suffix(".disabled") {
                Some(enabled_name) => current_path.with_file_name(enabled_name),
                None => return Ok(current_path),
            }
        } else if file_name.ends_with(".jar") {
            current_path.with_file_name(format!("{file_name}.disabled"))
        } else {
            return Ok(current_path);
        };

        let target_name = target_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ModError::InvalidModName(file_name.to_string()))?;
        if target_path.exists() {
            return Err(ModError::AlreadyInstalled(target_name.to_string()));
        }
        fs::rename(current_path, &target_path)?;
        self.rename_tracked_mod(instance_dir.as_ref(), file_name, target_name)?;
        Ok(target_path)
    }

    /// Removes an installed JAR after an explicit confirmation.
    pub fn remove_mod(
        &mut self,
        instance_dir: impl AsRef<Path>,
        file_name: &str,
        confirmed: bool,
    ) -> Result<(), ModError> {
        if !confirmed {
            return Err(ModError::ConfirmationRequired);
        }
        let instance_dir = instance_dir.as_ref();
        let mod_path = self.resolve_mod_path(instance_dir, file_name)?;
        fs::remove_file(mod_path)?;
        self.forget_tracked_mod(instance_dir, file_name)?;
        Ok(())
    }

    fn resolve_mod_path(&self, instance_dir: &Path, file_name: &str) -> Result<PathBuf, ModError> {
        if !is_mod_file_name(file_name) {
            return Err(ModError::InvalidModName(file_name.to_string()));
        }
        let mods_dir = instance_dir.join("mods");
        match fs::symlink_metadata(&mods_dir) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(ModError::InvalidModName("mods".to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ModError::NotFound(file_name.to_string()))
            }
            Err(error) => return Err(ModError::Io(error)),
        }
        let mod_path = mods_dir.join(file_name);
        match fs::symlink_metadata(&mod_path) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(mod_path),
            Ok(_) => Err(ModError::InvalidModName(file_name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Err(ModError::NotFound(file_name.to_string()))
            }
            Err(error) => Err(ModError::Io(error)),
        }
    }

    pub fn disable_mod(&mut self, jar_path: impl AsRef<Path>) -> Result<PathBuf, ModError> {
        let jar_path = jar_path.as_ref();
        if !jar_path.exists() {
            return Err(ModError::NotFound(jar_path.display().to_string()));
        }

        let file_name = jar_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ModError::InvalidMod(jar_path.display().to_string()))?;
        let disabled_path = jar_path.with_file_name(format!("{file_name}.disabled"));
        fs::rename(jar_path, &disabled_path)?;
        Ok(disabled_path)
    }

    pub fn enable_mod(&mut self, jar_path: impl AsRef<Path>) -> Result<PathBuf, ModError> {
        let jar_path = jar_path.as_ref();
        if !jar_path.exists() {
            return Err(ModError::NotFound(jar_path.display().to_string()));
        }

        let file_name = jar_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ModError::InvalidMod(jar_path.display().to_string()))?;
        let enabled_path = if let Some(name) = file_name.strip_suffix(".disabled") {
            jar_path.with_file_name(name)
        } else {
            jar_path.to_path_buf()
        };
        fs::rename(jar_path, &enabled_path)?;
        Ok(enabled_path)
    }
}

fn is_mod_file_name(file_name: &str) -> bool {
    (file_name.ends_with(".jar") || file_name.ends_with(".jar.disabled"))
        && file_name.len() <= 255
        && file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn write_tracked_manifest(
    mods_dir: &Path,
    manifest: &Path,
    entries: &[TrackedMod],
) -> Result<(), ModError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary_path = mods_dir.join(format!(".redstone-tracked-{nonce}.tmp"));
    fs::write(&temporary_path, serde_json::to_vec(entries)?)?;
    if let Err(error) = fs::rename(&temporary_path, manifest) {
        let _ = fs::remove_file(temporary_path);
        return Err(ModError::Io(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn mod_update_check_returns_newest_compatible_version_without_installing_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/project/sodium-project/version"))
            .and(query_param("game_versions", r#"["1.21.4"]"#))
            .and(query_param("loaders", r#"["fabric"]"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "version-2",
                    "version_number": "2.0.0",
                    "date_published": "2026-01-02T00:00:00Z",
                    "files": [{"url": "https://cdn.modrinth.com/sodium-2.jar", "filename": "sodium-2.jar", "primary": true}]
                },
                {
                    "id": "version-1",
                    "version_number": "1.0.0",
                    "date_published": "2025-01-02T00:00:00Z",
                    "files": [{"url": "https://cdn.modrinth.com/sodium-1.jar", "filename": "sodium-1.jar", "primary": true}]
                }
            ])))
            .mount(&server)
            .await;

        let client = ModrinthClient::with_base_url(server.uri());
        let update = client
            .check_mod_update(
                &TrackedMod {
                    project_id: "sodium-project".to_string(),
                    file_name: "sodium-1.jar".to_string(),
                    version_id: "version-1".to_string(),
                },
                "1.21.4",
                "fabric",
            )
            .await
            .unwrap();

        assert_eq!(update.unwrap().latest_version_id, "version-2");
    }

    #[tokio::test]
    async fn applying_an_update_requires_confirmation_and_replaces_the_tracked_jar() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/project/sodium-project/version"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": "version-2",
                    "version_number": "2.0.0",
                    "date_published": "2026-01-02T00:00:00Z",
                    "files": [{
                        "url": format!("{}/files/sodium-2.jar", server.uri()),
                        "filename": "sodium-2.jar",
                        "primary": true
                    }]
                }])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/files/sodium-2.jar"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"updated-jar".to_vec()))
            .mount(&server)
            .await;

        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        let mods_dir = instance_dir.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        fs::write(mods_dir.join("sodium-1.jar"), b"old-jar").unwrap();
        let tracked = TrackedMod {
            project_id: "sodium-project".to_string(),
            file_name: "sodium-1.jar".to_string(),
            version_id: "version-1".to_string(),
        };
        ModManager::new()
            .record_tracked_mod(&instance_dir, tracked.clone())
            .unwrap();

        let client = ModrinthClient::with_base_url(server.uri());
        let update = client
            .check_mod_update(&tracked, "1.21.4", "fabric")
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            client
                .apply_mod_update(&instance_dir, &update, "1.21.4", "fabric", false)
                .await,
            Err(ModError::ConfirmationRequired)
        ));
        assert_eq!(fs::read(mods_dir.join("sodium-1.jar")).unwrap(), b"old-jar");

        let updated = client
            .apply_mod_update(&instance_dir, &update, "1.21.4", "fabric", true)
            .await
            .unwrap();
        assert_eq!(updated.version_id, "version-2");
        assert_eq!(
            fs::read(mods_dir.join("sodium-2.jar")).unwrap(),
            b"updated-jar"
        );
        assert!(!mods_dir.join("sodium-1.jar").exists());
        assert_eq!(
            ModManager::new().tracked_mods(&instance_dir).unwrap(),
            vec![updated]
        );
    }

    #[tokio::test]
    async fn install_downloads_the_primary_compatible_modrinth_jar() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/project/sodium-project/version"))
            .and(query_param("game_versions", r#"["1.21.4"]"#))
            .and(query_param("loaders", r#"["fabric"]"#))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": "sodium-version-1",
                    "version_number": "1.0.0",
                    "date_published": "2025-01-02T00:00:00Z",
                    "files": [{
                        "url": format!("{}/files/sodium.jar", server.uri()),
                        "filename": "sodium.jar",
                        "primary": true
                    }]
                }])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/files/sodium.jar"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"real-jar-bytes".to_vec()))
            .mount(&server)
            .await;

        let root = tempdir().unwrap();
        let client = ModrinthClient::with_base_url(server.uri());
        let installed = client
            .install_compatible_mod(root.path(), "sodium-project", "1.21.4", "fabric")
            .await
            .unwrap();

        assert_eq!(installed.file_name().unwrap(), "sodium.jar");
        assert_eq!(fs::read(&installed).unwrap(), b"real-jar-bytes");
        assert!(matches!(
            client
                .install_compatible_mod(root.path(), "sodium-project", "1.21.4", "fabric")
                .await,
            Err(ModError::AlreadyInstalled(name)) if name == "sodium.jar"
        ));
        assert_eq!(fs::read(installed).unwrap(), b"real-jar-bytes");
    }

    #[tokio::test]
    async fn modrinth_search_uses_filters_and_parses_mocked_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/search"))
            .and(header("user-agent", "Redstone/0.1.0"))
            .and(query_param("query", "sodium"))
            .and(query_param(
                "facets",
                r#"[["project_type:mod"],["versions:1.21.4"],["categories:fabric"]]"#,
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "hits": [{
                    "project_id": "sodium-project",
                    "slug": "sodium",
                    "title": "Sodium",
                    "description": "A rendering optimization mod",
                    "author": "jellysquid3",
                    "downloads": 125000,
                    "icon_url": "https://cdn.example.test/sodium.png",
                    "project_type": "mod"
                }]
            })))
            .mount(&server)
            .await;

        let client = ModrinthClient::with_base_url(server.uri());
        let results = client.search("sodium", "1.21.4", "fabric").await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_id, "sodium-project");
        assert_eq!(results[0].title, "Sodium");
        assert_eq!(results[0].downloads, 125000);
    }

    #[test]
    fn disabling_a_mod_renames_the_jar_and_enabling_restores_it() {
        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        fs::create_dir_all(instance_dir.join("mods")).unwrap();

        let mod_path = instance_dir.join("mods").join("example.jar");
        fs::write(&mod_path, "mod").unwrap();

        let mut manager = ModManager::new();
        manager.disable_mod(&mod_path).unwrap();
        assert!(mod_path.with_extension("jar.disabled").exists());

        manager
            .enable_mod(mod_path.with_extension("jar.disabled"))
            .unwrap();
        assert!(mod_path.exists());
    }

    #[test]
    fn installed_mods_can_be_listed_toggled_and_removed_with_confirmation() {
        let root = tempdir().unwrap();
        let mods_dir = root.path().join("instance").join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        fs::write(mods_dir.join("alpha.jar"), "alpha").unwrap();
        fs::write(mods_dir.join("beta.jar.disabled"), "beta").unwrap();
        fs::write(mods_dir.join("notes.txt"), "not a mod").unwrap();

        let mut manager = ModManager::new();
        let listed = manager.list_mods(root.path().join("instance")).unwrap();
        assert_eq!(
            listed,
            vec![
                InstalledMod {
                    file_name: "alpha.jar".to_string(),
                    enabled: true
                },
                InstalledMod {
                    file_name: "beta.jar.disabled".to_string(),
                    enabled: false
                }
            ]
        );

        manager
            .set_mod_enabled(root.path().join("instance"), "alpha.jar", false)
            .unwrap();
        assert!(mods_dir.join("alpha.jar.disabled").exists());
        manager
            .set_mod_enabled(root.path().join("instance"), "alpha.jar.disabled", true)
            .unwrap();
        assert!(mods_dir.join("alpha.jar").exists());

        assert!(matches!(
            manager.remove_mod(root.path().join("instance"), "alpha.jar", false),
            Err(ModError::ConfirmationRequired)
        ));
        manager
            .remove_mod(root.path().join("instance"), "alpha.jar", true)
            .unwrap();
        assert!(!mods_dir.join("alpha.jar").exists());
        assert!(manager
            .set_mod_enabled(root.path().join("instance"), "../outside.jar", false)
            .is_err());
    }

    #[tokio::test]
    async fn curseforge_api_key_missing_error() {
        let client = CurseForgeClient::new(None);
        let result = client.search("jei", "1.21.4", "fabric").await;
        assert!(matches!(result, Err(ModError::CurseForgeApiKeyMissing)));
    }

    #[tokio::test]
    async fn curseforge_search_and_install_downloads_compatible_jar_and_tracks_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/search"))
            .and(header("x-api-key", "test-curseforge-key"))
            .and(query_param("gameId", "432"))
            .and(query_param("classId", "6"))
            .and(query_param("searchFilter", "appleskin"))
            .and(query_param("gameVersion", "1.21.4"))
            .and(query_param("modLoaderType", "4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": 12345,
                    "name": "AppleSkin",
                    "slug": "appleskin",
                    "summary": "Food HUD indicators",
                    "downloadCount": 98765.0,
                    "authors": [{"name": "squeek502"}],
                    "logo": {"thumbnailUrl": "https://media.forgecdn.net/logo.png"},
                    "links": {"websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/appleskin"},
                    "allowModDistribution": true
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/mods/12345"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 12345,
                    "name": "AppleSkin",
                    "slug": "appleskin",
                    "links": {"websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/appleskin"},
                    "allowModDistribution": true
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/mods/12345/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": 78910,
                    "modId": 12345,
                    "isAvailable": true,
                    "displayName": "AppleSkin 1.21.4",
                    "fileName": "appleskin-1.21.4.jar",
                    "downloadUrl": format!("{}/files/appleskin-1.21.4.jar", server.uri()),
                    "gameVersions": ["1.21.4", "Fabric"]
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/files/appleskin-1.21.4.jar"))
            .respond_with(
                ResponseTemplate::new(200).set_body_bytes(b"appleskin-binary-jar".to_vec()),
            )
            .mount(&server)
            .await;

        let client =
            CurseForgeClient::with_base_url(server.uri(), Some("test-curseforge-key".to_string()));
        let results = client
            .search("appleskin", "1.21.4", "fabric")
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_id, "curseforge-12345");
        assert_eq!(results[0].title, "AppleSkin");
        assert_eq!(results[0].source.as_deref(), Some("curseforge"));
        assert!(!results[0].distribution_disabled);

        let root = tempdir().unwrap();
        let installed = client
            .install_compatible_mod(root.path(), "curseforge-12345", "1.21.4", "fabric")
            .await
            .unwrap();

        assert_eq!(installed.file_name().unwrap(), "appleskin-1.21.4.jar");
        assert_eq!(fs::read(&installed).unwrap(), b"appleskin-binary-jar");

        let tracked = ModManager::new().tracked_mods(root.path()).unwrap();
        assert_eq!(tracked.len(), 1);
        assert_eq!(tracked[0].project_id, "curseforge-12345");
        assert_eq!(tracked[0].file_name, "appleskin-1.21.4.jar");
        assert_eq!(tracked[0].version_id, "78910");
    }

    #[tokio::test]
    async fn curseforge_search_is_cached_for_5_minutes_and_makes_no_second_network_call() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": 999,
                    "name": "CachedMod",
                    "slug": "cached-mod",
                    "summary": "Caching test",
                    "allowModDistribution": true
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key-xyz".to_string()));
        let first = client.search("cached", "1.21.4", "fabric").await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].title, "CachedMod");

        // Second call with same parameters hits in-memory cache with 5 min TTL
        let second = client.search("cached", "1.21.4", "fabric").await.unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].title, "CachedMod");
    }

    #[tokio::test]
    async fn curseforge_distribution_disabled_mod_refuses_download_and_returns_project_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/555"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 555,
                    "name": "NoDistMod",
                    "slug": "nodist-mod",
                    "links": {"websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/nodist-mod"},
                    "allowModDistribution": false
                }
            })))
            .mount(&server)
            .await;

        let root = tempdir().unwrap();
        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key".to_string()));
        let result = client
            .install_compatible_mod(root.path(), "curseforge-555", "1.21.4", "fabric")
            .await;

        match result {
            Err(ModError::DistributionDisabled { project_url }) => {
                assert_eq!(
                    project_url,
                    "https://www.curseforge.com/minecraft/mc-mods/nodist-mod"
                );
            }
            other => panic!("expected DistributionDisabled error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn curseforge_rate_limited_response_retries_with_backoff_and_reports_error_when_exhausted(
    ) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/search"))
            .respond_with(ResponseTemplate::new(429))
            .expect(3)
            .mount(&server)
            .await;

        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key".to_string()));
        let result = client.search("test", "1.21.4", "fabric").await;
        assert!(matches!(result, Err(ModError::CurseForgeRateLimited)));
    }

    #[tokio::test]
    async fn curseforge_malformed_json_returns_schema_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/search"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("{\"data\": \"not an array\"}"),
            )
            .mount(&server)
            .await;

        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key".to_string()));
        let result = client.search("test", "1.21.4", "fabric").await;
        assert!(matches!(result, Err(ModError::InvalidSchema(_))));
    }

    #[tokio::test]
    async fn curseforge_version_conflict_when_no_compatible_files_exist() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/777"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "id": 777,
                    "name": "ConflictMod",
                    "slug": "conflict-mod",
                    "allowModDistribution": true
                }
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/mods/777/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": []
            })))
            .mount(&server)
            .await;

        let root = tempdir().unwrap();
        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key".to_string()));
        let result = client
            .install_compatible_mod(root.path(), "curseforge-777", "1.21.4", "fabric")
            .await;
        assert!(matches!(result, Err(ModError::NoCompatibleFile)));
    }

    #[tokio::test]
    async fn curseforge_mod_update_check_and_apply() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/mods/123/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": 99999,
                    "modId": 123,
                    "isAvailable": true,
                    "displayName": "TestMod 2.0.0",
                    "fileName": "testmod-2.0.0.jar",
                    "downloadUrl": format!("{}/files/testmod-2.0.0.jar", server.uri()),
                    "gameVersions": ["1.21.4", "Fabric"]
                }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/files/testmod-2.0.0.jar"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"testmod-2-bytes".to_vec()))
            .mount(&server)
            .await;

        let root = tempdir().unwrap();
        let instance_dir = root.path().join("instance");
        let mods_dir = instance_dir.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        fs::write(mods_dir.join("testmod-1.0.0.jar"), b"testmod-1-bytes").unwrap();

        let tracked = TrackedMod {
            project_id: "curseforge-123".to_string(),
            file_name: "testmod-1.0.0.jar".to_string(),
            version_id: "10000".to_string(),
        };
        ModManager::new()
            .record_tracked_mod(&instance_dir, tracked.clone())
            .unwrap();

        let client = CurseForgeClient::with_base_url(server.uri(), Some("api-key".to_string()));
        let update = client
            .check_mod_update(&tracked, "1.21.4", "fabric")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(update.latest_version_id, "99999");
        assert_eq!(update.latest_file_name, "testmod-2.0.0.jar");

        // Fails without confirmation
        assert!(matches!(
            client
                .apply_mod_update(&instance_dir, &update, "1.21.4", "fabric", false)
                .await,
            Err(ModError::ConfirmationRequired)
        ));

        // Succeeds with confirmation
        let updated = client
            .apply_mod_update(&instance_dir, &update, "1.21.4", "fabric", true)
            .await
            .unwrap();
        assert_eq!(updated.version_id, "99999");
        assert_eq!(
            fs::read(mods_dir.join("testmod-2.0.0.jar")).unwrap(),
            b"testmod-2-bytes"
        );
        assert!(!mods_dir.join("testmod-1.0.0.jar").exists());
    }
}
