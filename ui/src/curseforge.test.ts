import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

let app: InstanceType<typeof App> | undefined;

describe('CurseForge API integration and catalog UI', () => {
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

  it('allows selecting CurseForge source, saving API key, and rendering distribution-disabled links', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    // Switch to Mods catalog tab
    await vi.waitFor(() => {
      const catalogTab = target.querySelector<HTMLButtonElement>('#tab-catalog');
      expect(catalogTab).not.toBeNull();
      expect(catalogTab!.textContent?.trim()).toBe('Mods');
      catalogTab!.click();
    });

    await vi.waitFor(() => {
      expect(target.querySelector('#panel-catalog')).not.toBeNull();
      expect(target.querySelector('.catalog-source-selector')).not.toBeNull();
    });

    // Select CurseForge source
    const cfSourceBtn = Array.from(
      target.querySelectorAll<HTMLButtonElement>('.catalog-source-btn')
    ).find((b) => b.textContent?.includes('CurseForge'));
    expect(cfSourceBtn).toBeDefined();
    cfSourceBtn!.click();

    // Verify CurseForge API key card is visible with explanatory hint
    await vi.waitFor(() => {
      expect(target.querySelector('.curseforge-key-section')).not.toBeNull();
      expect(target.textContent).toContain('CurseForge API Key (OS Keychain)');
      expect(target.textContent).toContain('Why is an API key required for CurseForge?');
      expect(target.querySelector('.key-locked-badge')).not.toBeNull();
      expect(target.querySelector<HTMLInputElement>('#panel-catalog input[type="search"]')?.disabled).toBe(true);
      expect(target.querySelector<HTMLButtonElement>('.category-drawer-item')?.disabled).toBe(true);
    });

    // Enter API key and save to keychain
    const keyInput = target.querySelector<HTMLInputElement>(
      '.curseforge-key-section input[type="password"], .curseforge-key-section input[type="text"]'
    );
    expect(keyInput).not.toBeNull();
    keyInput!.value = 'test-cf-secret-key-123';
    keyInput!.dispatchEvent(new Event('input', { bubbles: true }));

    const saveKeyBtn = target.querySelector<HTMLButtonElement>('.save-curseforge-key-btn');
    expect(saveKeyBtn).not.toBeNull();

    await vi.waitFor(() => {
      expect(saveKeyBtn!.disabled).toBe(false);
    });

    saveKeyBtn!.click();

    await vi.waitFor(() => {
      expect(target.textContent).toContain('CurseForge API key saved securely in OS keychain.');
      expect(storage.get('redstone.preview.secret.curseforge_api_key')).toBe('test-cf-secret-key-123');
    });

    // Search query for "optifine" or "jei"
    const searchInput = target.querySelector<HTMLInputElement>('#panel-catalog input[type="search"]');
    expect(searchInput).not.toBeNull();
    searchInput!.value = 'optifine';
    searchInput!.dispatchEvent(new Event('input', { bubbles: true }));

    // Verify distribution-disabled mod shows external link instead of install button
    await vi.waitFor(() => {
      expect(target.querySelector('.external-link-btn')).not.toBeNull();
      const link = target.querySelector<HTMLAnchorElement>('.external-link-btn');
      expect(link?.textContent).toContain('Distribution Disabled');
      expect(link?.href).toContain('curseforge.com/minecraft/mc-mods/optifine');
    });

    // Now search "jei" which has distribution enabled
    searchInput!.value = 'jei';
    searchInput!.dispatchEvent(new Event('input', { bubbles: true }));

    await vi.waitFor(() => {
      const installBtn = target.querySelector<HTMLButtonElement>('.install-mod-button');
      expect(installBtn).not.toBeNull();
      expect(installBtn?.textContent?.trim()).toBe('Install');
    });

    // Click install
    const installBtn = target.querySelector<HTMLButtonElement>('.install-mod-button');
    installBtn!.click();

    await vi.waitFor(() => {
      expect(confirm).toHaveBeenCalled();
      expect(target.textContent).toContain('jei.jar is a preview only');
    });
  });

  it('renders category chips and filters mods when a category is selected', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      const catalogTab = target.querySelector<HTMLButtonElement>('#tab-catalog');
      expect(catalogTab).not.toBeNull();
      expect(catalogTab!.textContent?.trim()).toBe('Mods');
      catalogTab!.click();
    });

    await vi.waitFor(() => {
      expect(target.querySelector('#panel-catalog')).not.toBeNull();
      expect(target.querySelector('.category-chip-bar')).not.toBeNull();
    });

    // Check that category chips exist
    const chips = Array.from(target.querySelectorAll<HTMLButtonElement>('.category-chip'));
    expect(chips.length).toBeGreaterThan(3);

    // Click "Optimization" category chip
    const optChip = chips.find((c) => c.textContent?.includes('Optimization'));
    expect(optChip).toBeDefined();
    optChip!.click();

    await vi.waitFor(() => {
      expect(optChip?.classList.contains('active')).toBe(true);
      expect(target.querySelectorAll('.mod-result').length).toBeGreaterThan(0);
      expect(target.textContent).toContain('Sodium');
    });
  });

  it('renders functioning SVG telemetry graphs in the monitor tab', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      const monitorTab = target.querySelector<HTMLButtonElement>('#tab-monitor');
      expect(monitorTab).not.toBeNull();
      monitorTab!.click();
    });

    await vi.waitFor(() => {
      expect(target.querySelector('#panel-monitor')).not.toBeNull();
    });

    // Check for the 4 functioning SVG graphs
    const graphBoxes = target.querySelectorAll('#panel-monitor .telemetry-graph-box');
    expect(graphBoxes.length).toBe(4);

    const svgElements = target.querySelectorAll('#panel-monitor .telemetry-svg');
    expect(svgElements.length).toBe(4);

    // Verify paths and legends exist
    svgElements.forEach((svg) => {
      expect(svg.querySelector('path')).not.toBeNull();
      expect(svg.querySelector('circle')).not.toBeNull();
    });

    // Verify Resource Telemetry divider exists between console and telemetry grid
    const divider = target.querySelector('#panel-monitor .monitor-section-divider');
    expect(divider).not.toBeNull();
    expect(divider?.textContent).toContain('Resource Telemetry');
    expect(divider?.textContent).toContain('Live Telemetry');
  });

  it('renders hamburger options menu with theme selector, preferences modal, shortcuts and about dialogs', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      expect(target.querySelector('.hamburger-btn')).not.toBeNull();
    });

    const hamburgerBtn = target.querySelector<HTMLButtonElement>('.hamburger-btn')!;
    hamburgerBtn.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.options-dropdown-menu')).not.toBeNull();
      expect(target.querySelectorAll('.theme-switch-btn').length).toBe(2);
    });

    // Test clicking light theme switch
    const lightThemeBtn = target.querySelector<HTMLButtonElement>('.theme-switch-btn.theme-light')!;
    lightThemeBtn.click();

    await vi.waitFor(() => {
      expect(storage.get('redstone.theme')).toBe('light');
    });

    // Click "Preferences" in the dropdown
    const prefMenuItem = Array.from(
      target.querySelectorAll<HTMLButtonElement>('.dropdown-menu-item')
    ).find((item) => item.textContent?.includes('Preferences'));
    expect(prefMenuItem).toBeDefined();
    prefMenuItem!.click();

    // Verify Preferences modal opened
    await vi.waitFor(() => {
      expect(target.querySelector('.preferences-dialog')).not.toBeNull();
      expect(target.textContent).toContain('Application');
      expect(target.textContent).toContain('Data folder');
      expect(target.textContent).toContain('Run in background');
      expect(target.textContent).toContain('Appearance');
      expect(target.textContent).toContain('Language');
      expect(target.textContent).toContain('Backups');
      expect(target.textContent).toContain('Auto backup on stop');
      expect(target.textContent).toContain('Maximum backups per server');
    });

    // Test auto-backup scope options chevron toggle
    const backupArrowBtn = target.querySelector<HTMLButtonElement>('.pref-arrow-btn')!;
    expect(backupArrowBtn).not.toBeNull();
    backupArrowBtn.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.pref-sub-options-row')).not.toBeNull();
      expect(target.textContent).toContain('Entire instance');
    });

    // Test stepper controls
    const stepperIncBtn = target.querySelector<HTMLButtonElement>('.stepper-btn[aria-label="Increase maximum backups"]')!;
    expect(stepperIncBtn).not.toBeNull();
    stepperIncBtn.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.stepper-count')?.textContent).toBe('1');
    });

    // Close preferences modal
    const closeBtn = target.querySelector<HTMLButtonElement>('.pref-close-btn')!;
    closeBtn.click();

    await vi.waitFor(() => {
      expect(target.querySelector('.preferences-dialog')).toBeNull();
    });
  });

  it('renders authentic category drawer matching image 3, project types with plugin guard, and environment toggles', async () => {
    const target = document.createElement('div');
    document.body.append(target);

    app = new App({ target });

    await vi.waitFor(() => {
      const catalogTab = target.querySelector<HTMLButtonElement>('#tab-catalog');
      expect(catalogTab).not.toBeNull();
      expect(catalogTab!.textContent?.trim()).toBe('Mods');
      catalogTab!.click();
    });

    await vi.waitFor(() => {
      expect(target.querySelector('#panel-catalog')).not.toBeNull();
      expect(target.querySelector('.catalog-category-sidebar')).not.toBeNull();
    });

    // Verify category header and collapse toggle
    const collapseBtn = target.querySelector<HTMLButtonElement>('.category-sidebar-header-btn')!;
    expect(collapseBtn).not.toBeNull();
    expect(collapseBtn.textContent).toContain('Category');

    // Verify the 19 authentic categories exist in the drawer
    const categoryItems = Array.from(target.querySelectorAll<HTMLButtonElement>('.category-drawer-item'));
    const categoryLabels = categoryItems.map((c) => c.textContent?.trim());
    expect(categoryLabels.some((l) => l?.includes('Adventure'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Cursed'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Decoration'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Equipment'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Food'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Game Mechanics'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Optimization'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Storage'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('Technology'))).toBe(true);
    expect(categoryLabels.some((l) => l?.includes('World Generation'))).toBe(true);

    // Verify official SVG logos exist
    expect(target.querySelector('.modrinth-logo')).not.toBeNull();
    expect(target.querySelector('.curseforge-logo')).not.toBeNull();

    // Verify Project Type tabs exist
    const projectTypePills = Array.from(target.querySelectorAll<HTMLButtonElement>('.project-type-pill'));
    expect(projectTypePills.some((p) => p.textContent?.includes('Mods'))).toBe(true);
    expect(projectTypePills.some((p) => p.textContent?.includes('Resource Packs'))).toBe(true);
    expect(projectTypePills.some((p) => p.textContent?.includes('Data Packs'))).toBe(true);
    expect(projectTypePills.some((p) => p.textContent?.includes('Plugins'))).toBe(true);

    // Verify Plugins tab is disabled when loader is Vanilla/Fabric/Forge
    const pluginsPill = projectTypePills.find((p) => p.textContent?.includes('Plugins'))!;
    expect(pluginsPill.disabled).toBe(true);
    expect(pluginsPill.textContent).toContain('Paper/Spigot only');

    // Test Resource Packs filter
    const rpPill = projectTypePills.find((p) => p.textContent?.includes('Resource Packs'))!;
    rpPill.click();

    await vi.waitFor(() => {
      expect(rpPill.classList.contains('active')).toBe(true);
      expect(target.textContent).toContain('Faithful 32x');
    });

    // Test Environment compatibility filter
    expect(target.querySelector('.env-toggle-container')).not.toBeNull();
    const envBtns = Array.from(target.querySelectorAll<HTMLButtonElement>('.env-toggle-btn'));
    expect(envBtns.map((b) => b.textContent?.trim())).toEqual(['All', 'Client', 'Server']);
    expect(target.querySelector('.env-toggle-container')?.textContent).not.toContain('Env:');
    const clientEnvBtn = envBtns.find((b) => b.textContent?.trim() === 'Client');
    expect(clientEnvBtn).toBeDefined();
    clientEnvBtn!.click();

    await vi.waitFor(() => {
      expect(clientEnvBtn!.classList.contains('active')).toBe(true);
    });
  });
});
