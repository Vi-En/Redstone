import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { generateOfflineUuid, readPlayerLists, writeBanlist, writeWhitelist } from './players';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('players adapter', () => {
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

  it('generates offline UUID from player name', () => {
    const uuid = generateOfflineUuid('Steve');
    expect(uuid).toMatch(/^[0-9a-f]{8}-0000-4000-8000-[0-9a-f]{12}$/);
  });

  it('reads preview player lists', async () => {
    const lists = await readPlayerLists('Survival');
    expect(lists.whitelist.length).toBeGreaterThan(0);
    expect(lists.whitelist[0].name).toBe('Alex');
    expect(lists.banlist.length).toBeGreaterThan(0);
    expect(lists.banlist[0].name).toBe('Griefer123');
  });

  it('writes whitelist and banlist in preview', async () => {
    await writeWhitelist('Survival', [{ name: 'Notch', uuid: '123' }]);
    let lists = await readPlayerLists('Survival');
    expect(lists.whitelist).toEqual([{ name: 'Notch', uuid: '123' }]);

    await writeBanlist('Survival', [{ name: 'BadActor', uuid: '456' }]);
    lists = await readPlayerLists('Survival');
    expect(lists.banlist).toEqual([{ name: 'BadActor', uuid: '456' }]);
  });

  it('invokes native Tauri commands when in Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({
      whitelist: [{ name: 'Player1', uuid: 'u1' }],
      banlist: []
    } as never);

    const lists = await readPlayerLists('Survival');
    expect(lists.whitelist).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith('read_player_lists', { instanceName: 'Survival' });

    await writeWhitelist('Survival', [{ name: 'Player2', uuid: 'u2' }]);
    expect(invoke).toHaveBeenCalledWith('write_whitelist', {
      instanceName: 'Survival',
      entries: [{ name: 'Player2', uuid: 'u2' }]
    });

    await writeBanlist('Survival', [{ name: 'Player3', uuid: 'u3' }]);
    expect(invoke).toHaveBeenCalledWith('write_banlist', {
      instanceName: 'Survival',
      entries: [{ name: 'Player3', uuid: 'u3' }]
    });
  });
});
