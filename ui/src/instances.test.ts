import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { createInstance, deleteInstance, duplicateInstance, listInstances } from './instances';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => false)
}));

describe('instance creation', () => {
  const storage = new Map<string, string>();

  afterEach(() => {
    storage.clear();
    vi.clearAllMocks();
    vi.mocked(isTauri).mockReturnValue(false);
    vi.unstubAllGlobals();
  });

  it('stores preview instance metadata locally', async () => {
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });

    const instance = await createInstance({
      name: 'Survival',
      minecraftVersion: '1.21.4',
      loader: 'vanilla'
    });

    expect(instance).toMatchObject({
      name: 'Survival',
      minecraft_version: '1.21.4',
      loader: 'vanilla',
      previewOnly: true
    });
  });

  it('calls the native instance command when running in Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({
      id: 1,
      name: 'Survival',
      minecraft_version: '1.21.4',
      loader: 'vanilla',
      path: '/app/instances/Survival',
      world: '/app/instances/Survival/world'
    });

    const instance = await createInstance({
      name: 'Survival',
      minecraftVersion: '1.21.4',
      loader: 'vanilla'
    });

    expect(invoke).toHaveBeenCalledWith('create_instance', {
      name: 'Survival',
      minecraftVersion: '1.21.4',
      loader: 'vanilla'
    });
    expect(instance.previewOnly).toBe(false);
  });

  it('lists locally stored preview instances', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => JSON.stringify([{
        id: 1,
        name: 'Survival',
        minecraft_version: '1.21.4',
        loader: 'fabric',
        previewOnly: true
      }]),
      setItem: vi.fn()
    });

    await expect(listInstances()).resolves.toMatchObject([{ name: 'Survival', previewOnly: true }]);
  });

  it('lists native instances through the Tauri command', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue([{
      id: 2,
      name: 'Creative',
      minecraft_version: '1.20.1',
      loader: 'fabric',
      path: '/app/instances/Creative',
      world: '/app/instances/Creative/world'
    }]);

    await expect(listInstances()).resolves.toMatchObject([{ name: 'Creative', previewOnly: false }]);
    expect(invoke).toHaveBeenCalledWith('list_instances');
  });

  it('duplicates and deletes instances with confirmation in preview and Tauri', async () => {
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });

    await createInstance({ name: 'Survival', minecraftVersion: '1.21.4', loader: 'fabric' });
    const dup = await duplicateInstance('Survival', 'Survival Copy');
    expect(dup).toMatchObject({
      name: 'Survival Copy',
      minecraft_version: '1.21.4',
      loader: 'fabric',
      previewOnly: true
    });

    await expect(deleteInstance('Survival Copy', false)).rejects.toThrow('confirmation');
    await deleteInstance('Survival Copy', true);
    const remaining = await listInstances();
    expect(remaining.map((i) => i.name)).toEqual(['Survival']);

    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce({
      id: 3,
      name: 'Native Copy',
      minecraft_version: '1.21.4',
      loader: 'fabric',
      path: '/app/instances/Native Copy',
      world: '/app/instances/Native Copy/world'
    });
    await duplicateInstance('Survival', 'Native Copy');
    expect(invoke).toHaveBeenCalledWith('duplicate_instance_by_name', {
      sourceName: 'Survival',
      newName: 'Native Copy'
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await deleteInstance('Native Copy', true);
    expect(invoke).toHaveBeenCalledWith('delete_instance_by_name', {
      name: 'Native Copy',
      confirmed: true
    });
  });
});