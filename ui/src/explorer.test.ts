import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { deleteFile, listDirectory, readFile, writeFile } from './explorer';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('explorer adapter', () => {
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

  it('lists preview files with directories first', async () => {
    const entries = await listDirectory('Survival');
    expect(entries.length).toBeGreaterThan(0);
    expect(entries[0].is_dir).toBe(true);
    expect(entries[0].name).toBe('logs');

    const subEntries = await listDirectory('Survival', 'logs');
    expect(subEntries).toHaveLength(1);
    expect(subEntries[0].name).toBe('latest.log');
    expect(subEntries[0].is_dir).toBe(false);
  });

  it('reads and writes preview files', async () => {
    const content = await readFile('Survival', 'server.properties');
    expect(content).toContain('server-port=25565');

    await writeFile('Survival', 'server.properties', 'server-port=25566\n');
    const updated = await readFile('Survival', 'server.properties');
    expect(updated).toBe('server-port=25566\n');
  });

  it('delete requires confirmation', async () => {
    await expect(deleteFile('Survival', 'server.properties', false)).rejects.toThrow(
      'Delete requires confirmation.'
    );
    await deleteFile('Survival', 'server.properties', true);
    await expect(readFile('Survival', 'server.properties')).rejects.toThrow('File not found');
  });

  it('invokes native Tauri commands when in Tauri', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue([
      { name: 'server.properties', relative_path: 'server.properties', is_dir: false, size_bytes: 50, modified_ms: 0 }
    ] as never);

    const entries = await listDirectory('Survival', '');
    expect(entries).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith('explorer_list_directory', {
      instanceName: 'Survival',
      relativePath: ''
    });

    vi.mocked(invoke).mockResolvedValue('content' as never);
    await readFile('Survival', 'server.properties');
    expect(invoke).toHaveBeenCalledWith('explorer_read_file', {
      instanceName: 'Survival',
      relativePath: 'server.properties'
    });

    await writeFile('Survival', 'test.txt', 'hello');
    expect(invoke).toHaveBeenCalledWith('explorer_write_file', {
      instanceName: 'Survival',
      relativePath: 'test.txt',
      contents: 'hello'
    });

    await deleteFile('Survival', 'test.txt', true);
    expect(invoke).toHaveBeenCalledWith('explorer_delete_file', {
      instanceName: 'Survival',
      relativePath: 'test.txt',
      confirmed: true
    });
  });
});
