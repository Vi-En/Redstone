import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type ServerStatus = 'running' | 'stopped' | 'crashed';

export interface ConsoleEvent {
  level: 'Info' | 'Warn' | 'Error';
  message: string;
  timestamp: number;
}

export interface CrashReportSummary {
  file_name: string;
  content: string;
}

export interface ServerSetupStatus {
  server_jar_present: boolean;
  eula_accepted: boolean;
  required_java_major: number;
  detected_java_major: number | null;
  latest_crash_report: CrashReportSummary | null;
  previewOnly: boolean;
}

export interface ProvisionedServer {
  minecraft_version: string;
  required_java_major: number;
  jar_size_bytes: number;
  previewOnly: boolean;
}

const previewStatusKey = 'redstone.preview.server.status';
const previewSetupKey = (instanceName: string) => `redstone.preview.server.setup.${instanceName}`;

function inferJavaMajor(minecraftVersion: string): number {
  const parts = minecraftVersion
    .split(/[^0-9]+/)
    .filter(Boolean)
    .map((p) => Number.parseInt(p, 10));
  const major = parts[0] ?? 1;
  const minor = parts[1] ?? 21;
  const patch = parts[2] ?? 0;
  if (major > 1 || minor >= 21 || (minor === 20 && patch >= 5)) return 21;
  if (minor >= 18) return 17;
  if (minor === 17) return 16;
  return 8;
}

function loadPreviewStatus(): Record<string, ServerStatus> {
  return JSON.parse(localStorage.getItem(previewStatusKey) ?? '{}') as Record<string, ServerStatus>;
}

function savePreviewStatus(status: Record<string, ServerStatus>): void {
  localStorage.setItem(previewStatusKey, JSON.stringify(status));
}

export async function serverStatus(instanceName: string): Promise<ServerStatus> {
  if (isTauri()) return invoke<ServerStatus>('server_status', { instanceName });
  return loadPreviewStatus()[instanceName] ?? 'stopped';
}

export async function startServer(instanceName: string): Promise<ServerStatus> {
  if (isTauri()) return invoke<ServerStatus>('start_server', { instanceName });
  const status = loadPreviewStatus();
  status[instanceName] = 'running';
  savePreviewStatus(status);
  return 'running';
}

export async function stopServer(instanceName: string): Promise<ServerStatus> {
  if (isTauri()) return invoke<ServerStatus>('stop_server', { instanceName });
  const status = loadPreviewStatus();
  status[instanceName] = 'stopped';
  savePreviewStatus(status);
  return 'stopped';
}

export async function sendServerCommand(instanceName: string, command: string): Promise<void> {
  if (isTauri()) {
    await invoke<void>('send_server_command', { instanceName, command });
    return;
  }
  if ((await serverStatus(instanceName)) !== 'running') throw new Error('Server is not running.');
}

export async function readServerConsole(instanceName: string): Promise<ConsoleEvent[]> {
  if (isTauri()) return invoke<ConsoleEvent[]>('read_server_console', { instanceName });
  return [];
}

export async function onServerConsole(
  instanceName: string,
  callback: (event: ConsoleEvent) => void
): Promise<UnlistenFn | undefined> {
  if (isTauri()) {
    return listen<{ instance_name: string; event: ConsoleEvent }>('server-console', (event) => {
      if (event.payload.instance_name === instanceName) {
        callback(event.payload.event);
      }
    });
  }
  return undefined;
}

export async function onServerStateChanged(
  instanceName: string,
  callback: (state: ServerStatus) => void
): Promise<UnlistenFn | undefined> {
  if (isTauri()) {
    return listen<{ instance_name: string; state: ServerStatus }>('server-state-changed', (event) => {
      if (event.payload.instance_name === instanceName) {
        callback(event.payload.state);
      }
    });
  }
  return undefined;
}

export async function serverSetupStatus(instanceName: string): Promise<ServerSetupStatus> {
  if (isTauri()) {
    const status = await invoke<Omit<ServerSetupStatus, 'previewOnly'>>('server_setup_status', {
      instanceName
    });
    return { ...status, previewOnly: false };
  }

  const saved = JSON.parse(localStorage.getItem(previewSetupKey(instanceName)) ?? 'null') as
    | ServerSetupStatus
    | null;
  return (
    saved ?? {
      server_jar_present: false,
      eula_accepted: false,
      required_java_major: 21,
      detected_java_major: 21,
      latest_crash_report: null,
      previewOnly: true
    }
  );
}

export async function downloadMojangServerJar(
  instanceName: string,
  minecraftVersion = '1.21.4'
): Promise<ProvisionedServer> {
  if (isTauri()) {
    const provisioned = await invoke<Omit<ProvisionedServer, 'previewOnly'>>(
      'download_mojang_server_jar',
      { instanceName }
    );
    return { ...provisioned, previewOnly: false };
  }

  const requiredJava = inferJavaMajor(minecraftVersion);
  const current = await serverSetupStatus(instanceName);
  const updated: ServerSetupStatus = {
    ...current,
    server_jar_present: true,
    required_java_major: requiredJava,
    previewOnly: true
  };
  localStorage.setItem(previewSetupKey(instanceName), JSON.stringify(updated));
  return {
    minecraft_version: minecraftVersion,
    required_java_major: requiredJava,
    jar_size_bytes: 51897902,
    previewOnly: true
  };
}

export async function acceptMojangEula(instanceName: string, confirmed: boolean): Promise<void> {
  if (!confirmed) {
    throw new Error('Accepting the Mojang EULA requires explicit confirmation.');
  }

  if (isTauri()) {
    await invoke<void>('accept_mojang_eula', { instanceName, confirmed });
    return;
  }

  const current = await serverSetupStatus(instanceName);
  const updated: ServerSetupStatus = {
    ...current,
    eula_accepted: true,
    previewOnly: true
  };
  localStorage.setItem(previewSetupKey(instanceName), JSON.stringify(updated));
}

