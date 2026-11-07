pub mod config;
pub mod console;
pub mod explorer;
pub mod instances;
pub mod jvm;
pub mod modloader;
pub mod mods;
pub mod monitor;
pub mod network;
pub mod players;
pub mod remote;
pub mod server;
pub mod worlds;

use crate::config::{Settings, SettingsStore};
use crate::console::ConsoleEvent;
use crate::explorer::{FileEntry, FileExplorer};
use crate::instances::{Instance, InstanceStore};
use crate::mods::{
    CurseForgeClient, InstalledMod, ModManager, ModSearchResult, ModUpdate, ModrinthClient,
    TrackedMod,
};
use crate::monitor::ResourceMonitor;
use crate::network::{
    CloudflareDnsClient, DesecClient, DnsRecord, DuckDnsClient, JoinInfo, NetworkManager,
    PlayitClient, SecretStore,
};
use crate::players::{PlayerEntry, PlayerManager};
use crate::remote::{PairingTicket, RemoteClient, RemoteController};
use crate::server::{
    accept_eula, infer_required_java_major, parse_java_major_version, read_latest_crash_report,
    MojangClient, ProvisionedServer, ServerSetupStatus,
};
use crate::worlds::{WorldConfig, WorldManager};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[tauri::command]
async fn load_settings(store: State<'_, SettingsStore>) -> Result<Settings, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.load())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_settings(store: State<'_, SettingsStore>, settings: Settings) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || store.save(&settings))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[derive(Clone)]
struct InstanceState {
    store: Arc<Mutex<InstanceStore>>,
    root_dir: PathBuf,
}

type InstanceMonitors = Arc<Mutex<HashMap<String, (ResourceMonitor, Option<f64>)>>>;

struct ActiveServer {
    child: tokio::sync::Mutex<Child>,
    pid: u32,
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    status: Arc<Mutex<String>>,
    stopping: Arc<Mutex<bool>>,
    last_cpu_sample: Arc<Mutex<Option<(std::time::Instant, u64)>>>,
    start_time: std::time::Instant,
}

#[derive(Clone, Default)]
struct ServerRuntime {
    servers: Arc<Mutex<HashMap<String, Arc<ActiveServer>>>>,
    events: Arc<Mutex<HashMap<String, Vec<ConsoleEvent>>>>,
    monitors: InstanceMonitors,
}

fn spawn_console_reader(
    app: Option<tauri::AppHandle>,
    instance_name: String,
    output: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    events: Arc<Mutex<HashMap<String, Vec<ConsoleEvent>>>>,
    monitors: InstanceMonitors,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(output).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let event = ConsoleEvent::from_line(&line);
            if let Ok(mut event_map) = events.lock() {
                let list = event_map.entry(instance_name.clone()).or_default();
                list.push(event.clone());
                if list.len() > 500 {
                    list.drain(..list.len() - 500);
                }
            }
            if let Ok(mut monitor_map) = monitors.lock() {
                let entry = monitor_map.entry(instance_name.clone()).or_insert_with(|| {
                    (
                        ResourceMonitor::new(crate::server::FakeClock::default(), 15.0, 10),
                        None,
                    )
                });
                if let Some(tps) = entry.0.parse_tps(&line) {
                    entry.1 = Some(tps);
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);
                    entry.0.record_sample(tps, now_ms);
                }
            }
            if let Some(ref app_handle) = app {
                let _ = app_handle.emit(
                    "server-console",
                    serde_json::json!({
                        "instance_name": &instance_name,
                        "event": &event,
                    }),
                );
            }
        }
    });
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct JvmSettings {
    ram_mb: u32,
    preset: String,
    custom: String,
}

impl Default for JvmSettings {
    fn default() -> Self {
        Self {
            ram_mb: 4096,
            preset: "Custom".to_string(),
            custom: String::new(),
        }
    }
}

impl JvmSettings {
    fn command_args(&self) -> Vec<String> {
        let mut args = vec![format!("-Xmx{}M", self.ram_mb)];
        if self.preset == "Performance" {
            args.extend([
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=200".to_string(),
                "-XX:+UnlockExperimentalVMOptions".to_string(),
                "-XX:+AlwaysPreTouch".to_string(),
                "-Dusing.aikars.flags=https://mcflags.emc.gs".to_string(),
                "-Daikars.new.flags=true".to_string(),
            ]);
        }
        if !self.custom.trim().is_empty() {
            args.extend(
                self.custom
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string),
            );
        }
        args
    }
}

impl InstanceState {
    fn instance_record(&self, name: &str) -> Result<Instance, String> {
        validate_instance_name(name)?;
        self.store
            .lock()
            .map_err(|error| format!("instance store lock failed: {error}"))?
            .list_instances()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|instance| instance.name == name)
            .ok_or_else(|| format!("instance does not exist: {name}"))
    }

    fn instance_dir(&self, name: &str) -> Result<PathBuf, String> {
        self.instance_record(name).map(|instance| instance.path)
    }

    fn create(
        &self,
        name: &str,
        minecraft_version: &str,
        loader: &str,
    ) -> Result<Instance, String> {
        let name = name.trim();
        validate_instance_name(name)?;
        validate_minecraft_version(minecraft_version)?;
        validate_loader(loader)?;

        let mut store = self
            .store
            .lock()
            .map_err(|error| format!("instance store lock failed: {error}"))?;
        if store
            .list_instances()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|instance| instance.name == name)
        {
            return Err(format!("instance name already exists: {name}"));
        }

        store
            .create_instance(name, minecraft_version, loader, self.root_dir.join(name))
            .map_err(|error| error.to_string())
    }

    fn duplicate(&self, source_name: &str, new_name: &str) -> Result<Instance, String> {
        let source_name = source_name.trim();
        let new_name = new_name.trim();
        validate_instance_name(source_name)?;
        validate_instance_name(new_name)?;
        self.store
            .lock()
            .map_err(|error| format!("instance store lock failed: {error}"))?
            .duplicate_named_instance(source_name, new_name)
            .map_err(|error| error.to_string())
    }

    fn delete(&self, name: &str, confirmed: bool) -> Result<(), String> {
        if !confirmed {
            return Err("deleting an instance requires confirmation".to_string());
        }
        let name = name.trim();
        validate_instance_name(name)?;
        self.store
            .lock()
            .map_err(|error| format!("instance store lock failed: {error}"))?
            .delete_instance(name)
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
async fn create_instance(
    state: State<'_, InstanceState>,
    name: String,
    minecraft_version: String,
    loader: String,
) -> Result<Instance, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.create(&name, &minecraft_version, &loader))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn duplicate_instance_by_name(
    state: State<'_, InstanceState>,
    source_name: String,
    new_name: String,
) -> Result<Instance, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.duplicate(&source_name, &new_name))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn delete_instance_by_name(
    state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    name: String,
    confirmed: bool,
) -> Result<(), String> {
    let is_running = runtime
        .servers
        .lock()
        .map_err(|error| format!("server runtime lock failed: {error}"))?
        .contains_key(name.trim());
    if is_running {
        return Err("stop the server before deleting this instance".to_string());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.delete(&name, confirmed))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn list_instances(state: State<'_, InstanceState>) -> Result<Vec<Instance>, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store
            .lock()
            .map_err(|error| format!("instance store lock failed: {error}"))?
            .list_instances()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn search_mods(
    query: String,
    minecraft_version: String,
    loader: String,
) -> Result<Vec<ModSearchResult>, String> {
    ModrinthClient::new()
        .search(&query, &minecraft_version, &loader)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn search_curseforge_mods(
    secrets: State<'_, SecretStore>,
    query: String,
    minecraft_version: String,
    loader: String,
) -> Result<Vec<ModSearchResult>, String> {
    let api_key = secrets
        .get("curseforge_api_key")
        .map_err(|e| e.to_string())?;
    CurseForgeClient::new(api_key)
        .search(&query, &minecraft_version, &loader)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_mod_from_curseforge(
    state: State<'_, InstanceState>,
    secrets: State<'_, SecretStore>,
    instance_name: String,
    project_id: String,
    minecraft_version: String,
    loader: String,
) -> Result<TrackedMod, String> {
    validate_minecraft_version(&minecraft_version)?;
    validate_loader(&loader)?;

    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    let tracking_dir = instance_dir.clone();
    let api_key = secrets
        .get("curseforge_api_key")
        .map_err(|e| e.to_string())?;
    let installed_path = CurseForgeClient::new(api_key)
        .install_compatible_mod(instance_dir, &project_id, &minecraft_version, &loader)
        .await
        .map_err(|error| error.to_string())?;
    let file_name = installed_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| "installed mod filename is not valid UTF-8".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        ModManager::new()
            .tracked_mods(tracking_dir)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|entry| entry.file_name == file_name)
            .ok_or_else(|| "installed mod tracking record was not written".to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn install_mod_from_modrinth(
    state: State<'_, InstanceState>,
    instance_name: String,
    project_id: String,
    minecraft_version: String,
    loader: String,
) -> Result<TrackedMod, String> {
    validate_minecraft_version(&minecraft_version)?;
    validate_loader(&loader)?;

    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    let tracking_dir = instance_dir.clone();
    let installed_path = ModrinthClient::new()
        .install_compatible_mod(instance_dir, &project_id, &minecraft_version, &loader)
        .await
        .map_err(|error| error.to_string())?;
    let file_name = installed_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| "installed mod filename is not valid UTF-8".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        ModManager::new()
            .tracked_mods(tracking_dir)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|entry| entry.file_name == file_name)
            .ok_or_else(|| "installed mod tracking record was not written".to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn check_mod_updates(
    state: State<'_, InstanceState>,
    secrets: State<'_, SecretStore>,
    instance_name: String,
    minecraft_version: String,
    loader: String,
) -> Result<Vec<ModUpdate>, String> {
    validate_minecraft_version(&minecraft_version)?;
    validate_loader(&loader)?;
    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;
    let tracking_dir = instance_dir.clone();
    let tracked =
        tauri::async_runtime::spawn_blocking(move || ModManager::new().tracked_mods(tracking_dir))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;

    let modrinth_client = ModrinthClient::new();
    let cf_api_key = secrets.get("curseforge_api_key").ok().flatten();
    let cf_client = CurseForgeClient::new(cf_api_key);
    let mut updates = Vec::new();
    for installed in tracked {
        if installed.project_id.starts_with("curseforge-")
            || installed.project_id.starts_with("cf-")
        {
            if let Ok(Some(update)) = cf_client
                .check_mod_update(&installed, &minecraft_version, &loader)
                .await
            {
                updates.push(update);
            }
        } else if let Some(update) = modrinth_client
            .check_mod_update(&installed, &minecraft_version, &loader)
            .await
            .map_err(|error| error.to_string())?
        {
            updates.push(update);
        }
    }
    Ok(updates)
}

#[tauri::command]
async fn apply_mod_update(
    state: State<'_, InstanceState>,
    secrets: State<'_, SecretStore>,
    instance_name: String,
    update: ModUpdate,
    minecraft_version: String,
    loader: String,
    confirmed: bool,
) -> Result<TrackedMod, String> {
    validate_minecraft_version(&minecraft_version)?;
    validate_loader(&loader)?;
    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;
    let tracking_dir = instance_dir.clone();
    let tracked =
        tauri::async_runtime::spawn_blocking(move || ModManager::new().tracked_mods(tracking_dir))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
    if !tracked.iter().any(|entry| {
        entry.project_id == update.project_id
            && entry.file_name == update.file_name
            && entry.version_id == update.current_version_id
    }) {
        return Err("installed mod changed; refresh the update list".to_string());
    }

    if update.project_id.starts_with("curseforge-") || update.project_id.starts_with("cf-") {
        let api_key = secrets
            .get("curseforge_api_key")
            .map_err(|e| e.to_string())?;
        CurseForgeClient::new(api_key)
            .apply_mod_update(
                instance_dir,
                &update,
                &minecraft_version,
                &loader,
                confirmed,
            )
            .await
            .map_err(|error| error.to_string())
    } else {
        ModrinthClient::new()
            .apply_mod_update(
                instance_dir,
                &update,
                &minecraft_version,
                &loader,
                confirmed,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
async fn list_installed_mods(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<Vec<InstalledMod>, String> {
    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    tauri::async_runtime::spawn_blocking(move || ModManager::new().list_mods(instance_dir))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn set_mod_enabled(
    state: State<'_, InstanceState>,
    instance_name: String,
    file_name: String,
    enabled: bool,
) -> Result<String, String> {
    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    let installed_path = tauri::async_runtime::spawn_blocking(move || {
        ModManager::new().set_mod_enabled(instance_dir, &file_name, enabled)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    installed_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| "mod filename is not valid UTF-8".to_string())
}

#[tauri::command]
async fn remove_mod(
    state: State<'_, InstanceState>,
    instance_name: String,
    file_name: String,
    confirmed: bool,
) -> Result<(), String> {
    let state = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    tauri::async_runtime::spawn_blocking(move || {
        ModManager::new().remove_mod(instance_dir, &file_name, confirmed)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn server_status(
    instance_state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
) -> Result<String, String> {
    instance_state.instance_dir(&instance_name)?;
    let server_opt = {
        let servers = runtime
            .servers
            .lock()
            .map_err(|error| format!("server runtime lock failed: {error}"))?;
        servers.get(&instance_name).cloned()
    };
    if let Some(server) = server_opt {
        let mut child = server.child.lock().await;
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            let status = server
                .status
                .lock()
                .map(|s| s.clone())
                .unwrap_or_else(|_| "running".to_string());
            Ok(status)
        } else {
            drop(child);
            if let Ok(mut servers) = runtime.servers.lock() {
                servers.remove(&instance_name);
            }
            Ok("stopped".to_string())
        }
    } else {
        Ok("stopped".to_string())
    }
}

#[tauri::command]
async fn start_server(
    app: tauri::AppHandle,
    instance_state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
) -> Result<String, String> {
    let instance_dir = instance_state.instance_dir(&instance_name)?;
    validate_server_jar(&instance_dir)?;
    let eula_path = instance_dir.join("eula.txt");
    let eula = tokio::fs::read_to_string(eula_path)
        .await
        .map_err(|_| "Accept the Mojang EULA in eula.txt before starting the server".to_string())?;
    validate_eula(&eula)?;

    let jvm_settings = match tokio::fs::read_to_string(instance_dir.join("jvm.json")).await {
        Ok(contents) => {
            serde_json::from_str::<JvmSettings>(&contents).map_err(|error| error.to_string())?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => JvmSettings::default(),
        Err(error) => return Err(error.to_string()),
    };
    let mut java_args = jvm_settings.command_args();
    java_args.extend([
        "-jar".to_string(),
        "server.jar".to_string(),
        "nogui".to_string(),
    ]);

    let existing_server = {
        let servers = runtime
            .servers
            .lock()
            .map_err(|error| format!("server runtime lock failed: {error}"))?;
        servers.get(&instance_name).cloned()
    };
    if let Some(server) = existing_server {
        let mut child = server.child.lock().await;
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Ok("running".to_string());
        }
        drop(child);
        if let Ok(mut servers) = runtime.servers.lock() {
            servers.remove(&instance_name);
        }
    }

    let mut child = Command::new("java")
        .args(java_args)
        .current_dir(instance_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start Java server: {error}"))?;

    let pid = child.id().unwrap_or(0);
    let stdin = child.stdin.take();

    let active_server = Arc::new(ActiveServer {
        child: tokio::sync::Mutex::new(child),
        pid,
        stdin: tokio::sync::Mutex::new(stdin),
        status: Arc::new(Mutex::new("running".to_string())),
        stopping: Arc::new(Mutex::new(false)),
        last_cpu_sample: Arc::new(Mutex::new(None)),
        start_time: std::time::Instant::now(),
    });

    {
        let mut child_guard = active_server.child.lock().await;
        if let Some(stdout) = child_guard.stdout.take() {
            spawn_console_reader(
                Some(app.clone()),
                instance_name.clone(),
                stdout,
                runtime.events.clone(),
                runtime.monitors.clone(),
            );
        }
        if let Some(stderr) = child_guard.stderr.take() {
            spawn_console_reader(
                Some(app.clone()),
                instance_name.clone(),
                stderr,
                runtime.events.clone(),
                runtime.monitors.clone(),
            );
        }
    }

    if let Ok(mut servers) = runtime.servers.lock() {
        servers.insert(instance_name.clone(), active_server.clone());
    }

    let servers_clone = runtime.servers.clone();
    let server_arc = active_server.clone();
    let app_clone = app.clone();
    let name_clone = instance_name.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let mut child_guard = server_arc.child.lock().await;
            match child_guard.try_wait() {
                Ok(Some(exit_status)) => {
                    let stopping = server_arc.stopping.lock().map(|s| *s).unwrap_or(false);
                    let new_state = if stopping || exit_status.success() {
                        "stopped"
                    } else {
                        "crashed"
                    };
                    if let Ok(mut status) = server_arc.status.lock() {
                        *status = new_state.to_string();
                    }
                    if let Ok(mut servers) = servers_clone.lock() {
                        servers.remove(&name_clone);
                    }
                    let _ = app_clone.emit(
                        "server-state-changed",
                        serde_json::json!({
                            "instance_name": name_clone,
                            "state": new_state,
                        }),
                    );
                    break;
                }
                Ok(None) => {}
                Err(_) => {
                    if let Ok(mut status) = server_arc.status.lock() {
                        *status = "crashed".to_string();
                    }
                    if let Ok(mut servers) = servers_clone.lock() {
                        servers.remove(&name_clone);
                    }
                    let _ = app_clone.emit(
                        "server-state-changed",
                        serde_json::json!({
                            "instance_name": name_clone,
                            "state": "crashed",
                        }),
                    );
                    break;
                }
            }
        }
    });

    let _ = app.emit(
        "server-state-changed",
        serde_json::json!({
            "instance_name": instance_name,
            "state": "running",
        }),
    );

    Ok("running".to_string())
}

#[tauri::command]
async fn read_server_console(
    instance_state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
) -> Result<Vec<ConsoleEvent>, String> {
    instance_state.instance_dir(&instance_name)?;
    let mut events = runtime
        .events
        .lock()
        .map_err(|error| format!("server console lock failed: {error}"))?;
    Ok(events.remove(&instance_name).unwrap_or_default())
}

#[tauri::command]
async fn stop_server(
    app: tauri::AppHandle,
    instance_state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
) -> Result<String, String> {
    instance_state.instance_dir(&instance_name)?;
    let server_opt = {
        let servers = runtime
            .servers
            .lock()
            .map_err(|error| format!("server runtime lock failed: {error}"))?;
        servers.get(&instance_name).cloned()
    };

    if let Some(server) = server_opt {
        if let Ok(mut stopping) = server.stopping.lock() {
            *stopping = true;
        }

        {
            let mut stdin_guard = server.stdin.lock().await;
            if let Some(stdin) = stdin_guard.as_mut() {
                let _ = stdin.write_all(b"stop\n").await;
                let _ = stdin.flush().await;
            }
        }

        let mut exited = false;
        {
            let mut child_guard = server.child.lock().await;
            for _ in 0..50 {
                if let Ok(Some(_)) = child_guard.try_wait() {
                    exited = true;
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            if !exited {
                let _ = child_guard.kill().await;
            }
        }

        if let Ok(mut status) = server.status.lock() {
            *status = "stopped".to_string();
        }
        if let Ok(mut servers) = runtime.servers.lock() {
            servers.remove(&instance_name);
        }

        let _ = app.emit(
            "server-state-changed",
            serde_json::json!({
                "instance_name": instance_name,
                "state": "stopped",
            }),
        );
    }
    Ok("stopped".to_string())
}

#[tauri::command]
async fn send_server_command(
    instance_state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
    command: String,
) -> Result<(), String> {
    instance_state.instance_dir(&instance_name)?;
    let command = command.trim();
    if command.is_empty() || command.contains('\n') || command.contains('\r') {
        return Err("command must be a single non-empty line".to_string());
    }
    let server = {
        let servers = runtime
            .servers
            .lock()
            .map_err(|error| format!("server runtime lock failed: {error}"))?;
        servers
            .get(&instance_name)
            .cloned()
            .ok_or_else(|| "server is not running".to_string())?
    };

    let mut stdin_guard = server.stdin.lock().await;
    let stdin = stdin_guard
        .as_mut()
        .ok_or_else(|| "server stdin is unavailable".to_string())?;
    stdin
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stdin.flush().await.map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn read_server_properties(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<BTreeMap<String, String>, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        WorldManager::new()
            .read_server_properties(instance_dir)
            .map(|config| config.properties)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn write_server_properties(
    state: State<'_, InstanceState>,
    instance_name: String,
    properties: BTreeMap<String, String>,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        WorldManager::new()
            .write_server_properties(instance_dir, &WorldConfig { properties })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn read_jvm_settings(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<JvmSettings, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    match tokio::fs::read_to_string(instance_dir.join("jvm.json")).await {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(JvmSettings::default()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
async fn write_jvm_settings(
    state: State<'_, InstanceState>,
    instance_name: String,
    settings: JvmSettings,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    if !(512..=262_144).contains(&settings.ram_mb) {
        return Err("RAM must be between 512 MiB and 262144 MiB".to_string());
    }
    tokio::fs::write(
        instance_dir.join("jvm.json"),
        serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?,
    )
    .await
    .map_err(|error| error.to_string())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResourceMetrics {
    cpu_percent: f64,
    memory_mb: u64,
    heap_used_mb: u64,
    heap_max_mb: u64,
    tps: Option<f64>,
    low_tps_alert: bool,
    pid: Option<u32>,
    uptime_seconds: Option<u64>,
    cpu_ticks: u64,
}

fn read_process_metrics_linux(pid: u32) -> (u64, u64) {
    if pid == 0 {
        return (0, 0);
    }
    let mut rss_kb = 0;
    if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    rss_kb = parts[1].parse::<u64>().unwrap_or(0);
                }
                break;
            }
        }
    }
    let mut total_ticks = 0;
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        if let Some(idx) = stat.rfind(')') {
            let rest = &stat[idx + 1..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            if fields.len() > 12 {
                let utime = fields[11].parse::<u64>().unwrap_or(0);
                let stime = fields[12].parse::<u64>().unwrap_or(0);
                total_ticks = utime + stime;
            }
        }
    }
    (rss_kb, total_ticks)
}

#[tauri::command]
async fn read_resource_metrics(
    state: State<'_, InstanceState>,
    runtime: State<'_, ServerRuntime>,
    instance_name: String,
) -> Result<ResourceMetrics, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    let jvm_settings = match tokio::fs::read_to_string(instance_dir.join("jvm.json")).await {
        Ok(contents) => serde_json::from_str::<JvmSettings>(&contents).unwrap_or_default(),
        Err(_) => JvmSettings::default(),
    };
    let heap_max_mb = jvm_settings.ram_mb as u64;

    let server_opt = {
        let servers = runtime
            .servers
            .lock()
            .map_err(|error| format!("server runtime lock failed: {error}"))?;
        servers.get(&instance_name).cloned()
    };

    let Some(server) = server_opt else {
        return Ok(ResourceMetrics {
            cpu_percent: 0.0,
            memory_mb: 0,
            heap_used_mb: 0,
            heap_max_mb,
            tps: None,
            low_tps_alert: false,
            pid: None,
            uptime_seconds: None,
            cpu_ticks: 0,
        });
    };

    let status = server.status.lock().map_err(|e| e.to_string())?.clone();
    if status != "running" {
        return Ok(ResourceMetrics {
            cpu_percent: 0.0,
            memory_mb: 0,
            heap_used_mb: 0,
            heap_max_mb,
            tps: None,
            low_tps_alert: false,
            pid: None,
            uptime_seconds: None,
            cpu_ticks: 0,
        });
    }

    let pid = server.pid;
    let (rss_kb, total_ticks) =
        tauri::async_runtime::spawn_blocking(move || read_process_metrics_linux(pid))
            .await
            .map_err(|e| e.to_string())?;

    let memory_mb = rss_kb / 1024;
    let heap_used_mb = memory_mb.min(heap_max_mb);

    let now = std::time::Instant::now();
    let cpu_percent = {
        let mut sample_guard = server.last_cpu_sample.lock().map_err(|e| e.to_string())?;
        if let Some((last_time, last_ticks)) = *sample_guard {
            let elapsed_secs = now.duration_since(last_time).as_secs_f64();
            if elapsed_secs >= 0.1 {
                let delta_ticks = total_ticks.saturating_sub(last_ticks);
                let cpu_secs = delta_ticks as f64 / 100.0;
                let calculated = (cpu_secs / elapsed_secs) * 100.0;
                *sample_guard = Some((now, total_ticks));
                (calculated * 10.0).round() / 10.0
            } else {
                0.0
            }
        } else {
            *sample_guard = Some((now, total_ticks));
            0.0
        }
    };

    let (tps, low_tps_alert) = {
        let monitors = runtime
            .monitors
            .lock()
            .map_err(|e| format!("monitors lock failed: {e}"))?;
        if let Some((monitor, last_tps)) = monitors.get(&instance_name) {
            (*last_tps, monitor.alert_active())
        } else {
            (None, false)
        }
    };

    let uptime_seconds = Some(server.start_time.elapsed().as_secs());

    Ok(ResourceMetrics {
        cpu_percent,
        memory_mb,
        heap_used_mb,
        heap_max_mb,
        tps,
        low_tps_alert,
        pid: Some(pid),
        uptime_seconds,
        cpu_ticks: total_ticks,
    })
}

#[tauri::command]
async fn explorer_list_directory(
    state: State<'_, InstanceState>,
    instance_name: String,
    relative_path: String,
) -> Result<Vec<FileEntry>, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        FileExplorer::new(instance_dir)
            .list_directory(&relative_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn explorer_read_file(
    state: State<'_, InstanceState>,
    instance_name: String,
    relative_path: String,
) -> Result<String, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        FileExplorer::new(instance_dir)
            .read_file(&relative_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn explorer_write_file(
    state: State<'_, InstanceState>,
    instance_name: String,
    relative_path: String,
    contents: String,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        FileExplorer::new(instance_dir)
            .write_file(&relative_path, &contents)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn explorer_delete_file(
    state: State<'_, InstanceState>,
    instance_name: String,
    relative_path: String,
    confirmed: bool,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        FileExplorer::new(instance_dir)
            .delete_file(&relative_path, confirmed)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlayerLists {
    whitelist: Vec<PlayerEntry>,
    banlist: Vec<PlayerEntry>,
}

#[tauri::command]
async fn read_player_lists(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<PlayerLists, String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let pm = PlayerManager::new();
        let whitelist = pm
            .read_whitelist(instance_dir.join("whitelist.json"))
            .map_err(|e| e.to_string())?;
        let banlist = pm
            .read_banlist(instance_dir.join("banned-players.json"))
            .map_err(|e| e.to_string())?;
        Ok(PlayerLists { whitelist, banlist })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn write_whitelist(
    state: State<'_, InstanceState>,
    instance_name: String,
    entries: Vec<PlayerEntry>,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        PlayerManager::new()
            .write_whitelist(instance_dir.join("whitelist.json"), &entries)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn write_banlist(
    state: State<'_, InstanceState>,
    instance_name: String,
    entries: Vec<PlayerEntry>,
) -> Result<(), String> {
    let instance_dir = state.instance_dir(&instance_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        PlayerManager::new()
            .write_banlist(instance_dir.join("banned-players.json"), &entries)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn get_join_info(
    settings: State<'_, SettingsStore>,
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<JoinInfo, String> {
    let current_settings = settings.load().unwrap_or_default();
    let instance_dir = state.instance_dir(&instance_name)?;

    let port = tauri::async_runtime::spawn_blocking(move || {
        WorldManager::new()
            .read_server_properties(instance_dir)
            .ok()
            .and_then(|config| {
                config
                    .properties
                    .get("server-port")
                    .and_then(|p| p.parse::<u16>().ok())
            })
            .unwrap_or(25565)
    })
    .await
    .map_err(|e| e.to_string())?;

    let local_ip = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("8.8.8.8:80")?;
            socket.local_addr()
        })
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let join_info = if current_settings.dns_provider.as_deref() == Some("playit")
        && !current_settings.lan_only
    {
        let sub = current_settings.dns_subdomain.as_deref().unwrap_or("play");
        let tunnel_host = format!("{sub}.gl.joinmc.link");
        JoinInfo {
            address: tunnel_host.clone(),
            port,
            is_lan: false,
            qr_payload: if port == 25565 {
                tunnel_host
            } else {
                format!("{tunnel_host}:{port}")
            },
        }
    } else {
        NetworkManager::compute_join_info(
            current_settings.lan_only,
            &local_ip,
            port,
            current_settings.dns_subdomain.as_deref(),
            current_settings.dns_domain.as_deref(),
        )
    };

    Ok(join_info)
}

#[tauri::command]
async fn save_network_secret(
    secrets: State<'_, SecretStore>,
    key: String,
    secret: String,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Secret key cannot be empty".to_string());
    }
    secrets.save(&key, &secret).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_network_secret(
    secrets: State<'_, SecretStore>,
    key: String,
) -> Result<Option<String>, String> {
    secrets.get(&key).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_network_secret(secrets: State<'_, SecretStore>, key: String) -> Result<(), String> {
    secrets.delete(&key).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_dns_records(
    secrets: State<'_, SecretStore>,
    provider: String,
    domain: String,
    token_key: String,
    zone_id: Option<String>,
    ip: Option<String>,
) -> Result<String, String> {
    let token = secrets
        .get(&token_key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No credential found in keychain for '{token_key}'"))?;

    if token.trim().is_empty() {
        return Err(format!("Credential for '{token_key}' is empty"));
    }

    match provider.to_lowercase().as_str() {
        "duckdns" => {
            let client = DuckDnsClient::default();
            client
                .update(&domain, &token, ip.as_deref())
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!("DuckDNS domain '{domain}' updated successfully"))
        }
        "cloudflare" => {
            let zone = zone_id.ok_or_else(|| "Zone ID required for Cloudflare".to_string())?;
            let client = CloudflareDnsClient::default();
            let record = DnsRecord {
                name: domain.clone(),
                r#type: "A".to_string(),
                value: ip.unwrap_or_else(|| "127.0.0.1".to_string()),
            };
            client
                .upsert_record(&zone, &token, &record)
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!(
                "Cloudflare record for '{domain}' updated successfully"
            ))
        }
        "desec" => {
            let client = DesecClient::default();
            let (subname, domain_name) = if let Some((sub, dom)) = domain.split_once('.') {
                (sub, dom)
            } else {
                ("", domain.as_str())
            };
            let target_ip = ip.unwrap_or_else(|| "127.0.0.1".to_string());
            client
                .update_record(domain_name, subname, &token, &target_ip)
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!("deSEC record for '{domain}' updated successfully"))
        }
        "playit" => {
            let client = PlayitClient::default();
            let tunnel_addr = client
                .sync_tunnel(&token, &domain)
                .await
                .map_err(|e| e.to_string())?;
            Ok(format!("Playit.gg tunnel '{tunnel_addr}' active"))
        }
        other => Err(format!("Unsupported DNS provider '{other}'")),
    }
}

fn validate_instance_name(name: &str) -> Result<(), String> {
    let character_count = name.chars().count();
    if name.trim().is_empty()
        || character_count > 48
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || " _-".contains(character))
    {
        return Err("instance name must be 1-48 letters, numbers, spaces, '_' or '-'".to_string());
    }

    Ok(())
}

fn validate_minecraft_version(version: &str) -> Result<(), String> {
    if version.is_empty()
        || version.len() > 32
        || !version.starts_with(|character: char| character.is_ascii_digit())
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte))
    {
        return Err("invalid Minecraft version".to_string());
    }

    Ok(())
}

fn validate_loader(loader: &str) -> Result<(), String> {
    if !["vanilla", "fabric", "forge", "neoforge"].contains(&loader) {
        return Err("unsupported mod loader".to_string());
    }

    Ok(())
}

fn validate_server_jar(instance_dir: &Path) -> Result<(), String> {
    if instance_dir.join("server.jar").is_file() {
        Ok(())
    } else {
        Err("server.jar is missing from this instance".to_string())
    }
}

fn validate_eula(contents: &str) -> Result<(), String> {
    if contents.lines().any(|line| line.trim() == "eula=true") {
        Ok(())
    } else {
        Err("Accept the Mojang EULA in eula.txt before starting the server".to_string())
    }
}

#[derive(Clone)]
pub struct RemoteState {
    pub controller: Arc<Mutex<RemoteController>>,
}

impl Default for RemoteState {
    fn default() -> Self {
        let host_name = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "host".to_string());
        let endpoint_id = format!("redstone-host-{}", host_name);
        Self {
            controller: Arc::new(Mutex::new(RemoteController::new(&endpoint_id))),
        }
    }
}

#[tauri::command]
async fn get_remote_pairing_ticket(state: State<'_, RemoteState>) -> Result<PairingTicket, String> {
    let mut controller = state
        .controller
        .lock()
        .map_err(|e| format!("Failed to lock remote controller: {e}"))?;
    Ok(controller.create_pairing_ticket())
}

#[tauri::command]
async fn list_paired_devices(state: State<'_, RemoteState>) -> Result<Vec<String>, String> {
    let controller = state
        .controller
        .lock()
        .map_err(|e| format!("Failed to lock remote controller: {e}"))?;
    Ok(controller.list_paired_clients())
}

#[tauri::command]
async fn revoke_paired_device(
    state: State<'_, RemoteState>,
    endpoint_id: String,
) -> Result<bool, String> {
    let mut controller = state
        .controller
        .lock()
        .map_err(|e| format!("Failed to lock remote controller: {e}"))?;
    Ok(controller.unpair_client(&endpoint_id))
}

#[tauri::command]
async fn pair_remote_device(
    state: State<'_, RemoteState>,
    client_endpoint_id: String,
    ticket_code: String,
) -> Result<RemoteClient, String> {
    let mut controller = state
        .controller
        .lock()
        .map_err(|e| format!("Failed to lock remote controller: {e}"))?;
    controller
        .pair_client(&client_endpoint_id, &ticket_code)
        .map_err(|e| e.to_string())
}

fn compute_setup_status(
    instance_dir: &Path,
    minecraft_version: &str,
    detected_java_major: Option<u32>,
) -> Result<ServerSetupStatus, String> {
    let server_jar_present = validate_server_jar(instance_dir).is_ok();
    let eula_accepted = std::fs::read_to_string(instance_dir.join("eula.txt"))
        .ok()
        .map(|contents| validate_eula(&contents).is_ok())
        .unwrap_or(false);
    let required_java_major = infer_required_java_major(minecraft_version);
    let latest_crash_report =
        read_latest_crash_report(instance_dir).map_err(|error| error.to_string())?;

    Ok(ServerSetupStatus {
        server_jar_present,
        eula_accepted,
        required_java_major,
        detected_java_major,
        latest_crash_report,
    })
}

#[tauri::command]
async fn server_setup_status(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<ServerSetupStatus, String> {
    let state_clone = state.inner().clone();
    let instance =
        tauri::async_runtime::spawn_blocking(move || state_clone.instance_record(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    let detected_java_major = match Command::new("java").arg("-version").output().await {
        Ok(out) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            );
            parse_java_major_version(&combined)
        }
        Err(_) => None,
    };

    tauri::async_runtime::spawn_blocking(move || {
        compute_setup_status(
            &instance.path,
            &instance.minecraft_version,
            detected_java_major,
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn download_mojang_server_jar(
    state: State<'_, InstanceState>,
    instance_name: String,
) -> Result<ProvisionedServer, String> {
    let state_clone = state.inner().clone();
    let instance =
        tauri::async_runtime::spawn_blocking(move || state_clone.instance_record(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    MojangClient::new()
        .download_server_jar(&instance.path, &instance.minecraft_version)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn accept_mojang_eula(
    state: State<'_, InstanceState>,
    instance_name: String,
    confirmed: bool,
) -> Result<(), String> {
    let state_clone = state.inner().clone();
    let instance_dir =
        tauri::async_runtime::spawn_blocking(move || state_clone.instance_dir(&instance_name))
            .await
            .map_err(|error| error.to_string())??;

    tauri::async_runtime::spawn_blocking(move || {
        accept_eula(instance_dir, confirmed).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.toml");
            app.manage(SettingsStore::new(settings_path));
            let app_data_dir = app.path().app_data_dir()?;
            let instances_dir = app_data_dir.join("instances");
            let store = InstanceStore::open(app_data_dir.join("instances.sqlite"), &instances_dir)?;
            app.manage(InstanceState {
                store: Arc::new(Mutex::new(store)),
                root_dir: instances_dir,
            });
            app.manage(ServerRuntime::default());
            app.manage(SecretStore::new());
            app.manage(RemoteState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            create_instance,
            duplicate_instance_by_name,
            delete_instance_by_name,
            list_instances,
            search_mods,
            search_curseforge_mods,
            install_mod_from_modrinth,
            install_mod_from_curseforge,
            check_mod_updates,
            apply_mod_update,
            list_installed_mods,
            set_mod_enabled,
            remove_mod,
            server_status,
            server_setup_status,
            download_mojang_server_jar,
            accept_mojang_eula,
            start_server,
            stop_server,
            send_server_command,
            read_server_console,
            read_server_properties,
            write_server_properties,
            read_jvm_settings,
            write_jvm_settings,
            read_resource_metrics,
            explorer_list_directory,
            explorer_read_file,
            explorer_write_file,
            explorer_delete_file,
            read_player_lists,
            write_whitelist,
            write_banlist,
            get_join_info,
            save_network_secret,
            get_network_secret,
            delete_network_secret,
            update_dns_records,
            get_remote_pairing_ticket,
            list_paired_devices,
            revoke_paired_device,
            pair_remote_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn instance_names_cannot_escape_the_instances_directory() {
        assert!(validate_instance_name("../outside").is_err());
        assert!(validate_instance_name(" ").is_err());
        assert!(validate_instance_name("Survival_01").is_ok());
    }

    #[test]
    fn versions_and_loaders_are_validated_before_creation() {
        assert!(validate_minecraft_version("1.21.4").is_ok());
        assert!(validate_minecraft_version("../../outside").is_err());
        assert!(validate_loader("fabric").is_ok());
        assert!(validate_loader("unknown").is_err());
    }

    #[test]
    fn creation_persists_metadata_and_builds_the_instance_directory() {
        let root = tempdir().unwrap();
        let instances_dir = root.path().join("instances");
        let store =
            InstanceStore::open(root.path().join("instances.sqlite"), &instances_dir).unwrap();
        let state = InstanceState {
            store: Arc::new(Mutex::new(store)),
            root_dir: instances_dir.clone(),
        };

        let instance = state.create("Survival", "1.21.4", "vanilla").unwrap();

        assert!(instance.path.starts_with(instances_dir));
        assert!(instance.world.is_dir());
        assert_eq!(
            state.store.lock().unwrap().list_instances().unwrap().len(),
            1
        );
        assert!(state.create("Survival", "1.21.4", "vanilla").is_err());
    }

    #[test]
    fn server_start_guards_require_jar_and_accepted_eula() {
        let root = tempdir().unwrap();
        assert!(validate_server_jar(root.path()).is_err());
        std::fs::write(root.path().join("server.jar"), "jar").unwrap();
        assert!(validate_server_jar(root.path()).is_ok());
        assert!(validate_eula("eula=false").is_err());
        assert!(validate_eula("# accepted\neula=true\n").is_ok());
    }

    #[test]
    fn read_process_metrics_handles_zero_and_current_pid() {
        assert_eq!(read_process_metrics_linux(0), (0, 0));
        let my_pid = std::process::id();
        let (rss_kb, _ticks) = read_process_metrics_linux(my_pid);
        assert!(rss_kb > 0);
    }

    #[test]
    fn secret_store_saves_and_retrieves_tokens() {
        let store = SecretStore::new();
        assert!(store.save("network_test_token", "abc123xyz").is_ok());
        assert_eq!(
            store.get("network_test_token").unwrap(),
            Some("abc123xyz".to_string())
        );
        assert!(store.delete("network_test_token").is_ok());
        assert_eq!(store.get("network_test_token").unwrap(), None);
    }

    #[test]
    fn remote_state_ticket_and_pairing_lifecycle() {
        let state = RemoteState::default();
        let ticket = {
            let mut ctrl = state.controller.lock().unwrap();
            ctrl.create_pairing_ticket()
        };
        assert!(!ticket.ticket_code.is_empty());
        assert!(ticket.qr_payload.contains("redstone://pair/"));

        {
            let mut ctrl = state.controller.lock().unwrap();
            let paired = ctrl.pair_client("device-alpha", &ticket.ticket_code);
            assert!(paired.is_ok());
            assert_eq!(ctrl.list_paired_clients(), vec!["device-alpha".to_string()]);
            assert!(ctrl.unpair_client("device-alpha"));
            assert_eq!(ctrl.list_paired_clients(), Vec::<String>::new());
        }
    }

    #[test]
    fn instance_duplicate_delete_and_setup_status_flow() {
        let root = tempdir().unwrap();
        let instances_dir = root.path().join("instances");
        let store =
            InstanceStore::open(root.path().join("instances.sqlite"), &instances_dir).unwrap();
        let state = InstanceState {
            store: Arc::new(Mutex::new(store)),
            root_dir: instances_dir,
        };

        let created = state.create("Survival", "1.21.4", "fabric").unwrap();
        let initial_status = compute_setup_status(&created.path, "1.21.4", Some(21)).unwrap();
        assert!(!initial_status.server_jar_present);
        assert!(!initial_status.eula_accepted);
        assert_eq!(initial_status.required_java_major, 21);

        std::fs::write(created.path.join("server.jar"), "jar").unwrap();
        accept_eula(&created.path, true).unwrap();
        let ready_status = compute_setup_status(&created.path, "1.21.4", Some(21)).unwrap();
        assert!(ready_status.server_jar_present);
        assert!(ready_status.eula_accepted);

        let dup = state.duplicate("Survival", "Survival_Copy").unwrap();
        assert_eq!(dup.minecraft_version, "1.21.4");
        assert_eq!(dup.loader, "fabric");
        assert!(dup.path.join("server.jar").exists());

        assert!(state.delete("Survival_Copy", false).is_err());
        state.delete("Survival_Copy", true).unwrap();
        assert!(state.instance_record("Survival_Copy").is_err());
    }

    #[tokio::test]
    async fn curseforge_keychain_secret_flow() {
        let store = SecretStore::new();
        assert!(store.save("curseforge_api_key", "cf-test-key-999").is_ok());
        let key = store.get("curseforge_api_key").unwrap().unwrap();
        assert_eq!(key, "cf-test-key-999");
        let _client = CurseForgeClient::new(Some(key));
        assert!(store.delete("curseforge_api_key").is_ok());
        assert_eq!(store.get("curseforge_api_key").unwrap(), None);
    }
}
