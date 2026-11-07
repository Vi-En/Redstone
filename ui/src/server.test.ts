import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  acceptMojangEula,
  downloadMojangServerJar,
  onServerConsole,
  onServerStateChanged,
  readServerConsole,
  sendServerCommand,
  serverSetupStatus,
  serverStatus,
  startServer,
  stopServer
} from './server';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('server adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
  });

  afterEach(() => vi.unstubAllGlobals());

  it('runs and stops a preview server without pretending to launch Java', async () => {
    await expect(serverStatus('Survival')).resolves.toBe('stopped');
    await expect(startServer('Survival')).resolves.toBe('running');
    await expect(sendServerCommand('Survival', 'say hello')).resolves.toBeUndefined();
    await expect(stopServer('Survival')).resolves.toBe('stopped');
  });

  it('uses native lifecycle commands', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue('running' as never);

    await serverStatus('Survival');
    await startServer('Survival');
    await stopServer('Survival');
    await sendServerCommand('Survival', 'say hello');
    await readServerConsole('Survival');

    expect(invoke).toHaveBeenNthCalledWith(1, 'server_status', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'start_server', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenNthCalledWith(3, 'stop_server', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenNthCalledWith(4, 'send_server_command', {
      instanceName: 'Survival',
      command: 'say hello'
    });
    expect(invoke).toHaveBeenNthCalledWith(5, 'read_server_console', { instanceName: 'Survival' });
  });

  it('registers console and state listeners in Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    const unlistenMock = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlistenMock as never);

    const onConsole = vi.fn();
    const onState = vi.fn();
    const unlistenConsole = await onServerConsole('Survival', onConsole);
    const unlistenState = await onServerStateChanged('Survival', onState);

    expect(listen).toHaveBeenCalledWith('server-console', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('server-state-changed', expect.any(Function));
    expect(unlistenConsole).toBe(unlistenMock);
    expect(unlistenState).toBe(unlistenMock);
  });

  it('provisions server.jar and requires explicit confirmation before accepting Mojang EULA', async () => {
    const initial = await serverSetupStatus('Survival');
    expect(initial.server_jar_present).toBe(false);
    expect(initial.eula_accepted).toBe(false);

    await expect(acceptMojangEula('Survival', false)).rejects.toThrow('confirmation');

    const provisioned = await downloadMojangServerJar('Survival', '1.21.4');
    expect(provisioned.required_java_major).toBe(21);
    await acceptMojangEula('Survival', true);

    const updated = await serverSetupStatus('Survival');
    expect(updated.server_jar_present).toBe(true);
    expect(updated.eula_accepted).toBe(true);

    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        server_jar_present: true,
        eula_accepted: false,
        required_java_major: 21,
        detected_java_major: 21,
        latest_crash_report: { file_name: 'crash-1.txt', content: 'boom' }
      } as never)
      .mockResolvedValueOnce({
        minecraft_version: '1.21.4',
        required_java_major: 21,
        jar_size_bytes: 1024
      } as never)
      .mockResolvedValueOnce(undefined as never);

    const nativeSetup = await serverSetupStatus('Survival');
    expect(nativeSetup.latest_crash_report?.file_name).toBe('crash-1.txt');
    await downloadMojangServerJar('Survival', '1.21.4');
    await acceptMojangEula('Survival', true);

    expect(invoke).toHaveBeenCalledWith('server_setup_status', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenCalledWith('download_mojang_server_jar', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenCalledWith('accept_mojang_eula', {
      instanceName: 'Survival',
      confirmed: true
    });
  });
});