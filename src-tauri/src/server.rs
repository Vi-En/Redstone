use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ServerError {
    #[error("server process failed: {0}")]
    ProcessFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FakeClock {
    now_ms: u64,
}

impl FakeClock {
    pub fn advance(&mut self, duration_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(duration_ms);
    }

    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

pub trait Process {
    fn start(&mut self) -> Result<(), ServerError>;
    fn send_stop(&mut self) -> Result<(), ServerError>;
    fn kill(&mut self);
    fn is_running(&self) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct FakeProcess {
    running: bool,
    stop_requested: bool,
    kill_requested: bool,
    crash_on_start: bool,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self {
            running: true,
            ..Default::default()
        }
    }

    pub fn crash_on_start(mut self) -> Self {
        self.crash_on_start = true;
        self
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub fn kill_requested(&self) -> bool {
        self.kill_requested
    }
}

impl Process for FakeProcess {
    fn start(&mut self) -> Result<(), ServerError> {
        self.stop_requested = false;
        self.kill_requested = false;
        if self.crash_on_start {
            self.running = false;
            return Err(ServerError::ProcessFailed("startup failed".to_string()));
        }
        self.running = true;
        Ok(())
    }

    fn send_stop(&mut self) -> Result<(), ServerError> {
        self.stop_requested = true;
        Ok(())
    }

    fn kill(&mut self) {
        self.running = false;
        self.kill_requested = true;
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

pub struct ServerManager<P = FakeProcess> {
    process: P,
    state: ServerState,
    clock: FakeClock,
    graceful_stop_ms: u64,
    crash_backoff_ms: u64,
}

impl<P: Process> ServerManager<P> {
    pub fn new(clock: FakeClock, process: P) -> Self {
        Self {
            process,
            state: ServerState::Stopped,
            clock,
            graceful_stop_ms: 250,
            crash_backoff_ms: 500,
        }
    }

    pub fn start(&mut self) -> Result<(), ServerError> {
        if self.state == ServerState::Running {
            return Ok(());
        }

        self.state = ServerState::Starting;
        let result = self.process.start();

        if let Err(err) = result {
            self.state = ServerState::Crashed;
            self.crash_backoff_ms = self.crash_backoff_ms.saturating_mul(2).max(250);
            return Err(err);
        }

        if self.process.is_running() {
            self.state = ServerState::Running;
            self.crash_backoff_ms = 0;
        } else {
            self.state = ServerState::Crashed;
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), ServerError> {
        if self.state != ServerState::Running {
            return Ok(());
        }

        self.state = ServerState::Stopping;
        self.process.send_stop()?;

        if self.process.is_running() && self.clock.now_ms() >= self.graceful_stop_ms {
            self.process.kill();
        }

        self.state = if self.process.is_running() {
            ServerState::Running
        } else {
            ServerState::Stopped
        };

        Ok(())
    }

    pub fn restart(&mut self) -> Result<(), ServerError> {
        self.stop()?;
        self.start()
    }

    pub fn state(&self) -> ServerState {
        self.state
    }

    pub fn process(&self) -> &P {
        &self.process
    }

    pub fn crash_backoff_ms(&self) -> u64 {
        self.crash_backoff_ms
    }
}

/// Errors returned while provisioning a Minecraft server directory or EULA.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("Mojang API request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Minecraft version not found in manifest: {0}")]
    VersionNotFound(String),
    #[error("no server JAR download entry for Minecraft {0}")]
    MissingServerDownload(String),
    #[error("unsafe download URL returned by manifest")]
    UnsafeDownloadUrl,
    #[error("server.jar already exists in this instance")]
    ServerJarAlreadyExists,
    #[error("downloaded server.jar size did not match manifest")]
    DownloadSizeMismatch,
    #[error("accepting the Mojang EULA requires explicit confirmation")]
    EulaConfirmationRequired,
}

/// Metadata returned after downloading `server.jar` from Mojang.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvisionedServer {
    pub minecraft_version: String,
    pub required_java_major: u32,
    pub jar_size_bytes: u64,
}

/// Summary of the newest crash report in `crash-reports/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashReportSummary {
    pub file_name: String,
    pub content: String,
}

/// Readiness and environment check for an instance's server setup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSetupStatus {
    pub server_jar_present: bool,
    pub eula_accepted: bool,
    pub required_java_major: u32,
    pub detected_java_major: Option<u32>,
    pub latest_crash_report: Option<CrashReportSummary>,
}

/// Parses a Java major version from `java -version` output.
pub fn parse_java_major_version(output: &str) -> Option<u32> {
    for line in output.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("version") {
            continue;
        }
        let quoted = if let Some((_, after_first)) = line.split_once('"') {
            after_first.split('"').next()
        } else {
            None
        };
        if let Some(raw_ver) = quoted {
            let mut parts = raw_ver
                .split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty());
            let first = parts.next()?.parse::<u32>().ok()?;
            if first == 1 {
                if let Some(second) = parts.next().and_then(|s| s.parse::<u32>().ok()) {
                    return Some(second);
                }
            }
            return Some(first);
        }
    }
    None
}

/// Infers the required Java major version for a given Minecraft release when offline.
pub fn infer_required_java_major(minecraft_version: &str) -> u32 {
    let nums: Vec<u32> = minecraft_version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    let major = nums.first().copied().unwrap_or(1);
    let minor = nums.get(1).copied().unwrap_or(21);
    let patch = nums.get(2).copied().unwrap_or(0);

    if major > 1 || minor >= 21 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

/// Reads the most recent crash report from `<instance_dir>/crash-reports/`.
pub fn read_latest_crash_report(
    instance_dir: impl AsRef<Path>,
) -> Result<Option<CrashReportSummary>, io::Error> {
    let crash_dir = instance_dir.as_ref().join("crash-reports");
    if !crash_dir.is_dir() {
        return Ok(None);
    }

    let mut report_names: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&crash_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".txt") || name.ends_with(".log") {
                report_names.push(name.to_string());
            }
        }
    }

    report_names.sort();
    let Some(latest_name) = report_names.pop() else {
        return Ok(None);
    };

    let raw = std::fs::read_to_string(crash_dir.join(&latest_name))?;
    let content = if raw.len() > 16_384 {
        let mut cut = 16_384;
        while !raw.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        format!("{}\n...[truncated]", &raw[..cut])
    } else {
        raw
    };

    Ok(Some(CrashReportSummary {
        file_name: latest_name,
        content,
    }))
}

/// Writes `eula=true` to `<instance_dir>/eula.txt` only when explicitly confirmed by the user.
pub fn accept_eula(instance_dir: impl AsRef<Path>, confirmed: bool) -> Result<(), ProvisionError> {
    if !confirmed {
        return Err(ProvisionError::EulaConfirmationRequired);
    }
    let dir = instance_dir.as_ref();
    std::fs::create_dir_all(dir)?;
    let eula_contents =
        "# Accepted by user in Redstone after reviewing https://www.minecraft.net/eula\neula=true\n";
    std::fs::write(dir.join("eula.txt"), eula_contents)?;
    Ok(())
}

#[derive(Deserialize)]
struct VersionManifestResponse {
    versions: Vec<VersionManifestEntry>,
}

#[derive(Deserialize)]
struct VersionManifestEntry {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionPackageResponse {
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersionInfo>,
    downloads: Option<VersionDownloads>,
}

#[derive(Deserialize)]
struct JavaVersionInfo {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

#[derive(Deserialize)]
struct VersionDownloads {
    server: Option<ServerArtifact>,
}

#[derive(Deserialize)]
struct ServerArtifact {
    url: String,
    size: Option<u64>,
}

const MOJANG_USER_AGENT: &str = "Redstone/0.1.0 (local-first-minecraft-host)";
const MAX_SERVER_JAR_BYTES: usize = 256 * 1024 * 1024;

/// Client for downloading official Minecraft `server.jar` releases from Mojang at runtime.
#[derive(Clone)]
pub struct MojangClient {
    client: reqwest::Client,
    base_url: String,
}

impl Default for MojangClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MojangClient {
    /// Creates a client pointing at Mojang's official version manifest service.
    pub fn new() -> Self {
        Self::with_base_url("https://piston-meta.mojang.com")
    }

    /// Creates a client using a custom base URL (for mocked tests).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    fn is_safe_mojang_url(&self, url: &str) -> bool {
        let base = self.base_url.trim_end_matches('/');
        url.starts_with(base)
            || url.starts_with("https://piston-meta.mojang.com/")
            || url.starts_with("https://piston-data.mojang.com/")
            || url.starts_with("https://launchermeta.mojang.com/")
            || url.starts_with("https://launcher.mojang.com/")
    }

    /// Resolves `minecraft_version` in Mojang's manifest and downloads `server.jar` into `instance_dir`.
    pub async fn download_server_jar(
        &self,
        instance_dir: impl AsRef<Path>,
        minecraft_version: &str,
    ) -> Result<ProvisionedServer, ProvisionError> {
        let instance_dir = instance_dir.as_ref();
        let final_jar = instance_dir.join("server.jar");
        if final_jar.exists() {
            return Err(ProvisionError::ServerJarAlreadyExists);
        }

        tokio::fs::create_dir_all(instance_dir).await?;

        let manifest_url = format!(
            "{}/mc/game/version_manifest_v2.json",
            self.base_url.trim_end_matches('/')
        );
        let manifest: VersionManifestResponse = self
            .client
            .get(&manifest_url)
            .header(reqwest::header::USER_AGENT, MOJANG_USER_AGENT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let entry = manifest
            .versions
            .into_iter()
            .find(|v| v.id == minecraft_version)
            .ok_or_else(|| ProvisionError::VersionNotFound(minecraft_version.to_string()))?;

        if !self.is_safe_mojang_url(&entry.url) {
            return Err(ProvisionError::UnsafeDownloadUrl);
        }

        let pkg: VersionPackageResponse = self
            .client
            .get(&entry.url)
            .header(reqwest::header::USER_AGENT, MOJANG_USER_AGENT)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let required_java_major = pkg
            .java_version
            .map(|j| j.major_version)
            .unwrap_or_else(|| infer_required_java_major(minecraft_version));

        let artifact = pkg
            .downloads
            .and_then(|d| d.server)
            .ok_or_else(|| ProvisionError::MissingServerDownload(minecraft_version.to_string()))?;

        if !self.is_safe_mojang_url(&artifact.url) {
            return Err(ProvisionError::UnsafeDownloadUrl);
        }

        let jar_bytes = self
            .client
            .get(&artifact.url)
            .header(reqwest::header::USER_AGENT, MOJANG_USER_AGENT)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        if jar_bytes.is_empty() || jar_bytes.len() > MAX_SERVER_JAR_BYTES {
            return Err(ProvisionError::DownloadSizeMismatch);
        }
        if let Some(expected_size) = artifact.size {
            if expected_size > 0 && jar_bytes.len() as u64 != expected_size {
                return Err(ProvisionError::DownloadSizeMismatch);
            }
        }

        let temp_jar = instance_dir.join(".server.jar.part");
        tokio::fs::write(&temp_jar, &jar_bytes).await?;
        if final_jar.exists() {
            let _ = tokio::fs::remove_file(&temp_jar).await;
            return Err(ProvisionError::ServerJarAlreadyExists);
        }
        tokio::fs::rename(&temp_jar, &final_jar).await?;

        Ok(ProvisionedServer {
            minecraft_version: minecraft_version.to_string(),
            required_java_major,
            jar_size_bytes: jar_bytes.len() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn stop_uses_graceful_stop_then_force_kill_after_timeout() {
        let mut clock = FakeClock::default();
        clock.advance(300);

        let mut manager = ServerManager::new(clock, FakeProcess::new());
        manager.start().unwrap();

        manager.stop().unwrap();

        assert!(manager.process().stop_requested());
        assert!(manager.process().kill_requested());
        assert_eq!(manager.state(), ServerState::Stopped);
    }

    #[test]
    fn start_tracks_crash_backoff_when_restart_is_needed() {
        let mut manager =
            ServerManager::new(FakeClock::default(), FakeProcess::new().crash_on_start());

        let result = manager.start();

        assert!(result.is_err());
        assert_eq!(manager.state(), ServerState::Crashed);
        assert!(manager.crash_backoff_ms() >= 250);
    }

    #[test]
    fn java_version_parser_and_inference_cover_modern_and_legacy_formats() {
        assert_eq!(
            parse_java_major_version("openjdk version \"21.0.4\" 2024-07-16\nOpenJDK 64-Bit"),
            Some(21)
        );
        assert_eq!(
            parse_java_major_version("java version \"1.8.0_392\"\nJava(TM) SE Runtime"),
            Some(8)
        );
        assert_eq!(parse_java_major_version("java version \"17\""), Some(17));
        assert_eq!(infer_required_java_major("1.21.4"), 21);
        assert_eq!(infer_required_java_major("1.20.1"), 17);
        assert_eq!(infer_required_java_major("1.16.5"), 8);
    }

    #[test]
    fn eula_acceptance_requires_confirmation_before_writing_file() {
        let dir = tempdir().unwrap();
        assert!(accept_eula(dir.path(), false).is_err());
        assert!(!dir.path().join("eula.txt").exists());

        accept_eula(dir.path(), true).unwrap();
        let written = std::fs::read_to_string(dir.path().join("eula.txt")).unwrap();
        assert!(written.lines().any(|line| line.trim() == "eula=true"));
    }

    #[test]
    fn latest_crash_report_returns_most_recent_report_when_present() {
        let dir = tempdir().unwrap();
        assert_eq!(read_latest_crash_report(dir.path()).unwrap(), None);

        let crash_dir = dir.path().join("crash-reports");
        std::fs::create_dir_all(&crash_dir).unwrap();
        std::fs::write(
            crash_dir.join("crash-2026-11-06_10.00.00-server.txt"),
            "older crash",
        )
        .unwrap();
        std::fs::write(
            crash_dir.join("crash-2026-11-07_12.00.00-server.txt"),
            "---- Minecraft Crash Report ----\nTime: 2026-11-07",
        )
        .unwrap();

        let latest = read_latest_crash_report(dir.path()).unwrap().unwrap();
        assert_eq!(latest.file_name, "crash-2026-11-07_12.00.00-server.txt");
        assert!(latest.content.contains("Minecraft Crash Report"));
    }

    #[tokio::test]
    async fn mojang_client_downloads_server_jar_and_reads_required_java_version() {
        let mock_server = MockServer::start().await;
        let jar_bytes = b"PK\x03\x04mock-minecraft-server-jar";

        let manifest_body = serde_json::json!({
            "versions": [
                {
                    "id": "1.21.4",
                    "url": format!("{}/v1/packages/abc/1.21.4.json", mock_server.uri())
                }
            ]
        });
        let version_body = serde_json::json!({
            "id": "1.21.4",
            "javaVersion": { "majorVersion": 21 },
            "downloads": {
                "server": {
                    "size": jar_bytes.len(),
                    "url": format!("{}/v1/objects/def/server.jar", mock_server.uri())
                }
            }
        });

        Mock::given(method("GET"))
            .and(path("/mc/game/version_manifest_v2.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(manifest_body))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/packages/abc/1.21.4.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(version_body))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/objects/def/server.jar"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(jar_bytes.to_vec()))
            .mount(&mock_server)
            .await;

        let dir = tempdir().unwrap();
        let client = MojangClient::with_base_url(mock_server.uri());
        let provisioned = client
            .download_server_jar(dir.path(), "1.21.4")
            .await
            .unwrap();

        assert_eq!(provisioned.minecraft_version, "1.21.4");
        assert_eq!(provisioned.required_java_major, 21);
        assert_eq!(provisioned.jar_size_bytes, jar_bytes.len() as u64);
        assert_eq!(
            std::fs::read(dir.path().join("server.jar")).unwrap(),
            jar_bytes
        );

        // Refuses to overwrite an existing server.jar
        assert!(matches!(
            client.download_server_jar(dir.path(), "1.21.4").await,
            Err(ProvisionError::ServerJarAlreadyExists)
        ));
    }
}
