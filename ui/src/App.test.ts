import { afterEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

let app: InstanceType<typeof App> | undefined;

afterEach(() => {
  app?.$destroy();
  app = undefined;
  document.body.innerHTML = '';
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe('app bootstrap', () => {
  it('leaves loading and renders setup after local settings resolve', async () => {
    const target = document.createElement('div');
    document.body.append(target);
    const storage = new Map<string, string>();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      clear: () => storage.clear()
    });
    vi.stubGlobal('confirm', vi.fn(() => true));
    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('.sidebar-servers')).not.toBeNull();
      expect(target.querySelector('.servers-panel-card')).not.toBeNull();
      expect(target.textContent).toContain('Servers');
    });
    expect(target.textContent).not.toContain('Loading local settings...');
    expect(target.querySelectorAll('[role="tab"]')).toHaveLength(6);
    expect(target.querySelector('#panel-catalog')).toBeNull();
    expect(target.querySelector('#panel-server')).not.toBeNull();

    // Verify World and server properties GUI in Server Management
    expect(target.querySelector('.world-properties-gui')).not.toBeNull();
    expect(target.textContent).toContain('World Identity & MOTD');
    expect(target.textContent).toContain('Game Rules & Modes');
    expect(target.textContent).toContain('Environment & Security Toggles');
    expect(target.textContent).toContain('Capacity & Distances');
    expect(target.textContent).toContain('Advanced & Mod Properties');

    // Verify Live server console and Monitor tab button are NOT in Server Management
    expect(target.querySelector('#panel-server .monitor-console-card')).toBeNull();
    expect(target.textContent).not.toContain('Monitor tab');

    // Test tab keyboard navigation
    target.querySelector<HTMLButtonElement>('#tab-server')!.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })
    );
    await vi.waitFor(() => {
      expect(target.querySelector('#tab-monitor')?.getAttribute('aria-selected')).toBe('true');
    });

    target.querySelector<HTMLButtonElement>('#tab-catalog')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('#panel-catalog')).not.toBeNull();
    });

    const search = target.querySelector<HTMLInputElement>('input[type="search"]');
    expect(search).not.toBeNull();
    search!.value = 'Sodium';
    search!.dispatchEvent(new Event('input', { bubbles: true }));

    await vi.waitFor(() => {
      expect(target.textContent).toContain('Sodium');
    });
    target.querySelector<HTMLButtonElement>('.install-mod-button')!.click();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('Installed');
      expect(target.textContent).toContain('no file was downloaded in the browser');
      expect(target.textContent).toContain('sodium.jar');
    });
    target.querySelector<HTMLButtonElement>('#tab-installed')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('.installed-mod-row')).not.toBeNull();
      expect(target.querySelector('.update-count')?.textContent).toBe('1');
    });
    target.querySelector<HTMLButtonElement>('.update-mod-button')!.click();
    await vi.waitFor(() => {
      expect(target.textContent).toContain('sodium-2.jar');
    });
    expect(confirm).toHaveBeenCalledTimes(2);
    target.querySelector<HTMLButtonElement>('#tab-installed')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('.installed-mod-row')?.textContent).toContain('sodium-2.jar');
      expect(target.querySelector('.update-count')).toBeNull();
    });
    target.querySelector<HTMLButtonElement>('.installed-action:not(.remove-action)')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('.installed-mod-row')?.textContent).toContain('Disabled');
    });

    target.querySelector<HTMLButtonElement>('.remove-action')!.click();
    await vi.waitFor(() => {
      expect(target.textContent).toContain('No mods installed.');
    });
    target.querySelector<HTMLButtonElement>('#tab-server')!.click();
    await vi.waitFor(() => {
      expect(target.textContent).toContain('Start server');
    });
    expect(confirm).toHaveBeenCalledTimes(3);

    target.querySelector<HTMLButtonElement>('#tab-files')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('#panel-files')).not.toBeNull();
      expect(target.textContent).toContain('File explorer');
      expect(target.textContent).toContain('server.properties');
    });

    target.querySelector<HTMLButtonElement>('#tab-players')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('#panel-players')).not.toBeNull();
      expect(target.textContent).toContain('Player management');
      expect(target.textContent).toContain('Alex');
      expect(target.textContent).toContain('Griefer123');
    });

    // Check networking / join address card in Server Management
    target.querySelector<HTMLButtonElement>('#tab-server')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('.join-card')).not.toBeNull();
      expect(target.textContent).toContain('Server Join Address');
      expect(target.textContent).toContain('LAN ONLY');
    });

    const qrButton = Array.from(target.querySelectorAll<HTMLButtonElement>('.join-actions button')).find(
      (b) => b.textContent?.includes('Show QR')
    );
    expect(qrButton).toBeDefined();
    qrButton!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('.qr-display svg')).not.toBeNull();
      expect(target.textContent).toContain('Hide QR');
    });

    // Check Monitor tab (telemetry, live server console, and refresh status)
    target.querySelector<HTMLButtonElement>('#tab-monitor')!.click();
    await vi.waitFor(() => {
      expect(target.querySelector('#panel-monitor')).not.toBeNull();
      expect(target.textContent).toContain('Server monitor & telemetry');
      expect(target.textContent).toContain('Server Ticks (TPS)');
      expect(target.textContent).toContain('Process ID');
      expect(target.textContent).toContain('Process Uptime');
      expect(target.textContent).toContain('CPU Usage');
      expect(target.textContent).toContain('RAM Allocation');
      // Live server console transferred to Monitor tab
      expect(target.querySelector('#panel-monitor .monitor-console-card')).not.toBeNull();
      expect(target.querySelector('#panel-monitor .monitor-terminal')).not.toBeNull();
      expect(target.textContent).toContain('Live server console');
      expect(target.textContent).toContain('Refresh status');
    });
  }, 20000);
});
