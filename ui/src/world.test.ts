import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readWorldProperties, writeWorldProperties } from './world';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('world properties adapter', () => {
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

  it('round-trips preview properties', async () => {
    const properties = await readWorldProperties('Survival');
    properties.difficulty = 'hard';
    await writeWorldProperties('Survival', properties);
    await expect(readWorldProperties('Survival')).resolves.toMatchObject({ difficulty: 'hard' });
  });

  it('uses native read and write commands', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({ difficulty: 'normal' } as never);

    await readWorldProperties('Survival');
    await writeWorldProperties('Survival', { difficulty: 'hard' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'read_server_properties', { instanceName: 'Survival' });
    expect(invoke).toHaveBeenNthCalledWith(2, 'write_server_properties', {
      instanceName: 'Survival',
      properties: { difficulty: 'hard' }
    });
  });
});
