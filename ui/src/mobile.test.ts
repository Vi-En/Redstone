import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

let app: InstanceType<typeof App> | undefined;

describe('remote pairing and mobile companion view', () => {
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

  it('renders desktop remote pairing card and handles ticket generation and revocation', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('#panel-server')).not.toBeNull();
      expect(target.querySelector('.remote-card')).not.toBeNull();
      expect(target.textContent).toContain('Mobile Remote Control (Iroh)');
      expect(target.textContent).toContain('END-TO-END ENCRYPTED');
      expect(target.textContent).toContain('Pixel 8 (preview)');
    });

    // Generate pairing ticket
    const pairBtn = Array.from(target.querySelectorAll<HTMLButtonElement>('.remote-top-actions button')).find(
      (b) => b.textContent?.includes('Pair Mobile Device')
    );
    expect(pairBtn).toBeDefined();
    pairBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.pairing-ticket-box')).not.toBeNull();
      expect(target.querySelector('.ticket-code-display strong')?.textContent).toBe('a1b2c3');
      expect(target.querySelector('.qr-display svg')).not.toBeNull();
      expect(target.textContent).toContain('redstone-preview-host');
    });

    // Revoke device
    const revokeBtn = target.querySelector<HTMLButtonElement>('.revoke-device-btn');
    expect(revokeBtn).not.toBeNull();
    revokeBtn!.click();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('access revoked');
      expect(target.textContent).toContain('No remote devices paired yet');
    });
  });

  it('switches between desktop host and mobile companion views with functional controls', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('.app-shell')).not.toBeNull();
    });

    // Click topbar "Mobile View" button
    const modeSwitchBtn = target.querySelector<HTMLButtonElement>('.mode-switch-btn');
    expect(modeSwitchBtn).not.toBeNull();
    modeSwitchBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.mobile-shell')).not.toBeNull();
      expect(target.textContent).toContain('REDSTONE');
      expect(target.textContent).toContain('REMOTE COMPANION');
      expect(target.textContent).toContain('Iroh Encrypted');
      expect(target.querySelector('#mobile-panel-server')).not.toBeNull();
    });

    // Verify Server tab controls & telemetry
    expect(target.textContent).toContain('Server Lifecycle');
    expect(target.textContent).toContain('Real-time Telemetry');
    expect(target.textContent).toContain('Tick Rate');
    expect(target.textContent).toContain('CPU Usage');

    // Switch to Console tab via bottom nav
    const consoleNav = target.querySelector<HTMLButtonElement>('#mobile-tab-console');
    expect(consoleNav).not.toBeNull();
    consoleNav!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('#mobile-panel-console')).not.toBeNull();
      expect(target.textContent).toContain('Live Console Stream');
      expect(target.querySelector('.mobile-terminal')).not.toBeNull();
      expect(target.querySelector('.mobile-cmd-input')).not.toBeNull();
    });

    // Quick command chip click
    const quickChip = target.querySelector<HTMLButtonElement>('.quick-cmd-chip');
    expect(quickChip).not.toBeNull();
    quickChip!.click();

    // Switch to Players tab via bottom nav
    const playersNav = target.querySelector<HTMLButtonElement>('#mobile-tab-players');
    expect(playersNav).not.toBeNull();
    playersNav!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('#mobile-panel-players')).not.toBeNull();
      expect(target.textContent).toContain('Whitelisted Players');
      expect(target.querySelector('.mobile-player-add-form')).not.toBeNull();
    });

    // Return to desktop host
    const exitBtn = target.querySelector<HTMLButtonElement>('.mobile-exit-btn');
    expect(exitBtn).not.toBeNull();
    exitBtn!.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.app-shell')).not.toBeNull();
      expect(target.querySelector('.mobile-shell')).toBeNull();
    });
  });
});
