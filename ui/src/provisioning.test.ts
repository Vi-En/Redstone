import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

let app: InstanceType<typeof App> | undefined;

describe('sidebar instance actions, runtime provisioning, EULA, and crash reports', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear()
    });
    vi.stubGlobal('confirm', vi.fn(() => true));
  });

  afterEach(() => {
    app?.$destroy();
    app = undefined;
    document.body.innerHTML = '';
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('duplicates and deletes an instance from the left Servers sidebar', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('.sidebar-servers')).not.toBeNull();
    });

    // Open inline create server form and create "Alpha"
    const addBtn = target.querySelector<HTMLButtonElement>('button[aria-label="Add Server"]');
    expect(addBtn).not.toBeNull();
    addBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.server-inline-create')).not.toBeNull();
    });

    const nameInput = target.querySelector<HTMLInputElement>('.server-inline-create input[type="text"]');
    expect(nameInput).not.toBeNull();
    nameInput!.value = 'Alpha';
    nameInput!.dispatchEvent(new Event('input', { bubbles: true }));

    const createBtn = Array.from(
      target.querySelectorAll<HTMLButtonElement>('.server-create-actions button')
    ).find((b) => b.textContent?.includes('Create Server'));
    expect(createBtn).toBeDefined();
    createBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelectorAll('.server-item-card')).toHaveLength(1);
      expect(target.textContent).toContain('Alpha');
      expect(target.querySelector('.duplicate-instance-btn')).not.toBeNull();
      expect(target.querySelector('.delete-instance-btn')).not.toBeNull();
    });

    // Duplicate "Alpha" -> creates and selects "Alpha copy"
    const dupBtn = target.querySelector<HTMLButtonElement>('.duplicate-instance-btn');
    dupBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelectorAll('.server-item-card')).toHaveLength(2);
      expect(target.textContent).toContain('Alpha copy');
      expect(target.querySelector('#server-status-title')?.textContent).toBe('Alpha copy');
    });

    // Delete "Alpha copy" -> asks for confirmation and falls back to "Alpha"
    const delBtn = target.querySelector<HTMLButtonElement>('.delete-instance-btn');
    delBtn!.click();

    await vi.waitFor(() => {
      expect(confirm).toHaveBeenCalledTimes(1);
      expect(target.querySelectorAll('.server-item-card')).toHaveLength(1);
      expect(target.querySelector('#server-status-title')?.textContent).toBe('Alpha');
    });
  });

  it('downloads server.jar, enforces explicit EULA checkbox confirmation, and surfaces crash reports', async () => {
    storage.set(
      'redstone.preview.instances',
      JSON.stringify([
        {
          id: 101,
          name: 'Survival',
          minecraft_version: '1.21.4',
          loader: 'vanilla',
          previewOnly: true
        }
      ])
    );
    storage.set(
      'redstone.settings',
      JSON.stringify({
        onboarding_complete: true,
        lan_only: true,
        dns_provider: null,
        dns_subdomain: null,
        dns_domain: null,
        instance_name: 'Survival'
      })
    );
    storage.set(
      'redstone.preview.server.setup.Survival',
      JSON.stringify({
        server_jar_present: false,
        eula_accepted: false,
        required_java_major: 21,
        detected_java_major: 21,
        latest_crash_report: {
          file_name: 'crash-2026-11-07-server.txt',
          content: '---- Minecraft Crash Report ----\nDescription: Watching Server'
        },
        previewOnly: true
      })
    );

    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('.provisioning-card')).not.toBeNull();
      expect(target.textContent).toContain('Java 21+');
      expect(target.querySelector('.crash-report-box')).not.toBeNull();
      expect(target.textContent).toContain('crash-2026-11-07-server.txt');
      expect(target.textContent).toContain('Watching Server');
    });

    const downloadBtn = target.querySelector<HTMLButtonElement>('.download-jar-btn');
    const eulaCheckbox = target.querySelector<HTMLInputElement>('.eula-confirm-checkbox');
    const acceptEulaBtn = target.querySelector<HTMLButtonElement>('.accept-eula-btn');

    expect(downloadBtn).not.toBeNull();
    expect(eulaCheckbox).not.toBeNull();
    expect(acceptEulaBtn).not.toBeNull();
    expect(acceptEulaBtn!.disabled).toBe(true);

    // Download server.jar
    downloadBtn!.click();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('READY');
    });

    // Check explicit EULA confirmation checkbox, then click Accept EULA
    eulaCheckbox!.checked = true;
    eulaCheckbox!.dispatchEvent(new Event('change', { bubbles: true }));

    await vi.waitFor(() => {
      expect(acceptEulaBtn!.disabled).toBe(false);
    });

    acceptEulaBtn!.click();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('ACCEPTED');
    });
  });
});
