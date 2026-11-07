import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { defaultSettings, loadSettings, saveSettings } from './settings';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => false)
}));

describe('settings persistence', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      clear: () => storage.clear()
    });
  });

  afterEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    vi.mocked(isTauri).mockReturnValue(false);
    vi.unstubAllGlobals();
  });

  it('uses safe defaults in the browser preview', async () => {
    await expect(loadSettings()).resolves.toEqual(defaultSettings);
  });

  it('round-trips settings in the browser preview', async () => {
    const settings = { ...defaultSettings, onboarding_complete: true, lan_only: false };

    await saveSettings(settings);

    await expect(loadSettings()).resolves.toEqual(settings);
  });

  it('uses the native settings commands inside Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue(defaultSettings);

    await expect(loadSettings()).resolves.toEqual(defaultSettings);

    expect(invoke).toHaveBeenCalledWith('load_settings');
  });

  it('rejects invalid DNS host labels before persistence', async () => {
    await expect(saveSettings({
      ...defaultSettings,
      dns_subdomain: '../outside'
    })).rejects.toThrow('DNS subdomain must be a single valid hostname label.');
  });
});