import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readJvmSettings, systemMemoryMb, writeJvmSettings } from './jvm';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('JVM settings adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
    vi.stubGlobal('navigator', { deviceMemory: 8 });
  });

  afterEach(() => vi.unstubAllGlobals());

  it('round-trips preview JVM settings and detects memory', async () => {
    await writeJvmSettings('Survival', { ramMb: 6144, preset: 'Performance', custom: '' });
    await expect(readJvmSettings('Survival')).resolves.toEqual({
      ramMb: 6144,
      preset: 'Performance',
      custom: ''
    });
    expect(systemMemoryMb()).toBe(8192);
  });

  it('uses native settings commands', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({ ramMb: 4096, preset: 'Custom', custom: '' } as never);

    await readJvmSettings('Survival');
    await writeJvmSettings('Survival', { ramMb: 4096, preset: 'Custom', custom: '-Xms2G' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'read_jvm_settings', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'write_jvm_settings', {
      instanceName: 'Survival',
      settings: { ramMb: 4096, preset: 'Custom', custom: '-Xms2G' }
    });
  });
});
