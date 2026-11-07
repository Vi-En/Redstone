import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createDebouncer,
  applyModUpdate,
  checkModUpdates,
  getCurseForgeApiKey,
  installMod,
  listInstalledMods,
  removeMod,
  saveCurseForgeApiKey,
  searchMods,
  setModEnabled
} from './mods';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn()
}));

const sampleMod = {
  project_id: 'sample-project',
  slug: 'sample-mod',
  title: 'Sample Mod',
  description: 'A sample mod for tests.',
  author: 'Redstone',
  downloads: 123,
  icon_url: null
};

describe('mod search adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    storage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns matching sample results in browser preview', async () => {
    const response = await searchMods('sodium', '1.21.4', 'fabric');

    expect(response.previewOnly).toBe(true);
    expect(response.results.map((mod) => mod.slug)).toEqual(['sodium']);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('filters browser preview mods by selected category', async () => {
    const response = await searchMods('', '1.21.4', 'fabric', 'modrinth', 'Optimization');

    expect(response.previewOnly).toBe(true);
    expect(response.results.length).toBeGreaterThan(0);
    expect(response.results.every((mod) => mod.categories?.some((c) => c.toLowerCase().includes('optimization')))).toBe(true);
  });

  it('invokes the native catalog command with compatibility filters', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue([sampleMod] as never);

    const response = await searchMods('sample', '1.21.4', 'neoforge');

    expect(invoke).toHaveBeenCalledWith('search_mods', {
      query: 'sample',
      minecraftVersion: '1.21.4',
      loader: 'neoforge'
    });
    expect(response).toEqual({ results: [sampleMod], previewOnly: false });
  });

  it('uses the install command for native instances', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({
      project_id: 'sample-project',
      file_name: 'sample-mod.jar',
      version_id: 'version-1'
    } as never);

    const response = await installMod('Survival', sampleMod, '1.21.4', 'fabric');

    expect(invoke).toHaveBeenCalledWith('install_mod_from_modrinth', {
      instanceName: 'Survival',
      projectId: 'sample-project',
      minecraftVersion: '1.21.4',
      loader: 'fabric'
    });
    expect(response).toEqual({ fileName: 'sample-mod.jar', versionId: 'version-1', previewOnly: false });
  });

  it('marks browser preview installs as metadata-only', async () => {
    const response = await installMod('Survival', sampleMod, '1.21.4', 'fabric');

    expect(response).toEqual({ fileName: 'sample-mod.jar', versionId: 'preview-1', previewOnly: true });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('counts preview updates and only replaces a mod after confirmation', async () => {
    const sodium = { ...sampleMod, project_id: 'preview-sodium', slug: 'sodium' };
    await installMod('Survival', sodium, '1.21.4', 'fabric');
    const [update] = await checkModUpdates('Survival', '1.21.4', 'fabric');
    expect(update.latest_version_number).toBe('2.0.0');
    await expect(applyModUpdate('Survival', update, '1.21.4', 'fabric', false)).rejects.toThrow(
      'Applying a mod update requires confirmation.'
    );
    expect((await listInstalledMods('Survival')).mods[0].file_name).toBe('sodium.jar');

    await applyModUpdate('Survival', update, '1.21.4', 'fabric', true);

    expect((await listInstalledMods('Survival')).mods[0].file_name).toBe('sodium-2.jar');
    expect(await checkModUpdates('Survival', '1.21.4', 'fabric')).toEqual([]);
  });

  it('uses native update check and apply commands with confirmation', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    const update = {
      project_id: 'sample-project',
      file_name: 'sample-mod.jar',
      current_version_id: 'version-1',
      latest_version_id: 'version-2',
      latest_version_number: '2.0.0',
      latest_file_name: 'sample-mod-2.jar'
    };
    vi.mocked(invoke).mockResolvedValueOnce([update] as never).mockResolvedValueOnce({
      project_id: 'sample-project',
      file_name: 'sample-mod-2.jar',
      version_id: 'version-2'
    } as never);

    await expect(checkModUpdates('Survival', '1.21.4', 'fabric')).resolves.toEqual([update]);
    await applyModUpdate('Survival', update, '1.21.4', 'fabric', true);

    expect(invoke).toHaveBeenNthCalledWith(1, 'check_mod_updates', {
      instanceName: 'Survival',
      minecraftVersion: '1.21.4',
      loader: 'fabric'
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'apply_mod_update', {
      instanceName: 'Survival',
      update,
      minecraftVersion: '1.21.4',
      loader: 'fabric',
      confirmed: true
    });
  });

  it('lists, disables, enables, and removes preview installs', async () => {
    await installMod('Survival', sampleMod, '1.21.4', 'fabric');
    expect(await listInstalledMods('Survival')).toEqual({
      mods: [{ file_name: 'sample-mod.jar', enabled: true }],
      previewOnly: true
    });

    await setModEnabled('Survival', 'sample-mod.jar', false);
    expect((await listInstalledMods('Survival')).mods[0].enabled).toBe(false);
    await setModEnabled('Survival', 'sample-mod.jar', true);
    await expect(removeMod('Survival', 'sample-mod.jar', false)).rejects.toThrow(
      'Removing a mod requires confirmation.'
    );
    await removeMod('Survival', 'sample-mod.jar', true);
    expect((await listInstalledMods('Survival')).mods).toEqual([]);
  });

  it('sends native list, toggle, and confirmed removal commands', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue([] as never);

    await listInstalledMods('Survival');
    await setModEnabled('Survival', 'sample-mod.jar', false);
    await removeMod('Survival', 'sample-mod.jar', true);

    expect(invoke).toHaveBeenNthCalledWith(1, 'list_installed_mods', {
      instanceName: 'Survival'
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'set_mod_enabled', {
      instanceName: 'Survival',
      fileName: 'sample-mod.jar',
      enabled: false
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'remove_mod', {
      instanceName: 'Survival',
      fileName: 'sample-mod.jar',
      confirmed: true
    });
  });
});

describe('mod catalog debounce', () => {
  afterEach(() => vi.useRealTimers());

  it('runs once after the most recent query settles', () => {
    vi.useFakeTimers();
    const search = vi.fn();
    const debouncer = createDebouncer(search, 250);

    debouncer.schedule();
    vi.advanceTimersByTime(180);
    debouncer.schedule();
    vi.advanceTimersByTime(249);
    expect(search).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(search).toHaveBeenCalledOnce();
  });

  it('cancels a pending search when the query becomes too short', () => {
    vi.useFakeTimers();
    const search = vi.fn();
    const debouncer = createDebouncer(search, 250);

    debouncer.schedule();
    debouncer.cancel();
    vi.advanceTimersByTime(250);

    expect(search).not.toHaveBeenCalled();
  });
});

describe('curseforge adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    storage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear()
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('searches and installs mods from CurseForge in browser preview and native mode', async () => {
    // Browser preview search
    const previewRes = await searchMods('jei', '1.21.4', 'fabric', 'curseforge');
    expect(previewRes.previewOnly).toBe(true);
    expect(previewRes.results.map((m) => m.slug)).toContain('jei');

    // Native search
    vi.mocked(isTauri).mockReturnValue(true);
    const cfMod = {
      project_id: 'curseforge-238222',
      slug: 'jei',
      title: 'Just Enough Items (JEI)',
      description: 'View Items and Recipes in Minecraft.',
      author: 'mezz',
      downloads: 250000000,
      icon_url: null,
      source: 'curseforge' as const,
      project_url: 'https://www.curseforge.com/minecraft/mc-mods/jei',
      distribution_disabled: false
    };
    vi.mocked(invoke).mockResolvedValueOnce([cfMod] as never);

    const nativeRes = await searchMods('jei', '1.21.4', 'fabric', 'curseforge');
    expect(invoke).toHaveBeenCalledWith('search_curseforge_mods', {
      query: 'jei',
      minecraftVersion: '1.21.4',
      loader: 'fabric'
    });
    expect(nativeRes.results).toEqual([cfMod]);

    // Native install
    vi.mocked(invoke).mockResolvedValueOnce({
      project_id: 'curseforge-238222',
      file_name: 'jei-1.21.4.jar',
      version_id: '78910'
    } as never);

    const installRes = await installMod('Survival', cfMod, '1.21.4', 'fabric');
    expect(invoke).toHaveBeenCalledWith('install_mod_from_curseforge', {
      instanceName: 'Survival',
      projectId: 'curseforge-238222',
      minecraftVersion: '1.21.4',
      loader: 'fabric'
    });
    expect(installRes).toEqual({
      fileName: 'jei-1.21.4.jar',
      versionId: '78910',
      previewOnly: false
    });
  });

  it('refuses to install distribution-disabled CurseForge mods and surfaces project URL', async () => {
    const disabledMod = {
      project_id: 'curseforge-999999',
      slug: 'optifine',
      title: 'OptiFine',
      description: 'Distribution disabled',
      author: 'sp614x',
      downloads: 80000000,
      icon_url: null,
      source: 'curseforge' as const,
      project_url: 'https://www.curseforge.com/minecraft/mc-mods/optifine',
      distribution_disabled: true
    };

    await expect(installMod('Survival', disabledMod, '1.21.4', 'fabric')).rejects.toThrow(
      'Mod distribution is disabled by author: https://www.curseforge.com/minecraft/mc-mods/optifine'
    );
  });

  it('saves and loads CurseForge API key to OS keychain', async () => {
    await saveCurseForgeApiKey('cf-api-key-12345');
    const loaded = await getCurseForgeApiKey();
    expect(loaded).toBe('cf-api-key-12345');
  });
});