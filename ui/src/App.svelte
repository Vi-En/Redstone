<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import {
    createInstance,
    deleteInstance,
    duplicateInstance,
    listInstances,
    type CreatedInstance
  } from './instances';
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
    setModEnabled,
    type InstalledMod,
    type ModUpdate,
    type ModSearchResult,
    type ProjectType,
    type EnvironmentFilter
  } from './mods';
  import { defaultSettings, loadSettings, saveSettings, type Settings } from './settings';
  import {
    acceptMojangEula,
    downloadMojangServerJar,
    onServerConsole,
    onServerStateChanged,
    readServerConsole,
    sendServerCommand,
    serverSetupStatus,
    serverStatus,
    startServer,
    stopServer,
    type ConsoleEvent,
    type ServerSetupStatus,
    type ServerStatus
  } from './server';
  import { readWorldProperties, writeWorldProperties } from './world';
  import { readJvmSettings, systemMemoryMb, writeJvmSettings, type JvmSettings } from './jvm';
  import { formatUptime, readResourceMetrics, type ResourceMetrics } from './monitor';
  import {
    deleteFile as explorerDeleteFile,
    listDirectory as explorerListDirectory,
    readFile as explorerReadFile,
    writeFile as explorerWriteFile,
    type FileEntry
  } from './explorer';
  import {
    generateOfflineUuid,
    readPlayerLists,
    writeBanlist,
    writeWhitelist,
    type PlayerEntry,
    type PlayerLists
  } from './players';
  import {
    getJoinInfo,
    getNetworkSecret,
    saveNetworkSecret,
    updateDnsRecords,
    type JoinInfo
  } from './network';
  import { generateQrSvg } from './qrcode';
  import {
    getRemotePairingTicket,
    listPairedDevices,
    pairRemoteDevice,
    revokePairedDevice,
    type PairingTicket
  } from './remote';

  type WorkspaceTab = 'catalog' | 'installed' | 'server' | 'monitor' | 'files' | 'players';
  const workspaceTabs: WorkspaceTab[] = ['catalog', 'installed', 'server', 'monitor', 'files', 'players'];

  let showCreateInstanceForm = false;

  function avatarColorFor(name: string): string {
    const palette = [
      'linear-gradient(135deg, #df563f, #993322)',
      'linear-gradient(135deg, #b45309, #78350f)',
      'linear-gradient(135deg, #14866a, #0b4e3e)',
      'linear-gradient(135deg, #2563eb, #1d4ed8)',
      'linear-gradient(135deg, #7c3aed, #5b21b6)',
      'linear-gradient(135deg, #d97706, #b45309)'
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash += name.charCodeAt(i);
    return palette[hash % palette.length];
  }

  function formatLoader(ldr: string): string {
    if (ldr === 'neoforge') return 'NeoForge';
    if (ldr === 'forge') return 'Forge';
    if (ldr === 'fabric') return 'Fabric';
    return 'Vanilla';
  }

  function avatarInitialsFor(name: string): string {
    const parts = name.trim().split(/\s+/);
    if (parts.length >= 2) {
      return (parts[0][0] + parts[1][0]).toUpperCase();
    }
    return name.slice(0, 2).toUpperCase();
  }

  function clearConsole() {
    consoleEvents = [];
  }

  let settings: Settings | null = { ...defaultSettings };
  let activeTab: WorkspaceTab = (() => {
    if (typeof window !== 'undefined') {
      const paramTab = new URLSearchParams(window.location.search).get('tab') as WorkspaceTab | null;
      if (paramTab && workspaceTabs.includes(paramTab)) return paramTab;
    }
    return 'server';
  })();
  let instances: CreatedInstance[] = [];
  let instanceName = 'Survival';
  let minecraftVersion = '1.21.4';
  let loader = 'vanilla';
  let createdInstance: CreatedInstance | null = null;
  let loading = false;
  let saving = false;
  let saved = false;
  let error = '';
  let modQuery = '';
  let modCatalogSource: 'modrinth' | 'curseforge' = (() => {
    if (typeof window !== 'undefined') {
      const paramSource = new URLSearchParams(window.location.search).get('source');
      if (paramSource === 'curseforge' || paramSource === 'modrinth') return paramSource;
    }
    return 'modrinth';
  })();
  let modCatalogCategory = 'all';
  let curseforgeApiKey = '';
  let curseforgeApiKeySaved = false;
  let curseforgeKeyBusy = false;
  let curseforgeKeyMasked = true;
  let curseforgeKeyMessage = '';
  let curseforgeKeyError = '';

  $: isCurseForgeKeyRequired = modCatalogSource === 'curseforge' && !curseforgeApiKeySaved;

  // Hamburger and Options Modals State
  let showOptionsDropdown = false;
  let showPreferencesModal = false;
  let showShortcutsModal = false;
  let showAboutModal = false;
  let themeMode: 'system' | 'light' | 'dark' = (() => {
    if (typeof window !== 'undefined') {
      const paramTheme = new URLSearchParams(window.location.search).get('theme');
      if (paramTheme === 'light' || paramTheme === 'dark' || paramTheme === 'system') return paramTheme;
    }
    return typeof localStorage !== 'undefined'
      ? (localStorage.getItem('redstone.theme') as 'system' | 'light' | 'dark') || 'dark'
      : 'dark';
  })();

  function setTheme(mode: 'system' | 'light' | 'dark') {
    themeMode = mode;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('redstone.theme', mode);
    }
    if (typeof document !== 'undefined') {
      document.documentElement.setAttribute('data-theme', mode);
    }
  }

  onMount(() => {
    setTheme(themeMode);
  });

  // Preferences State (matching user uploaded preferences dialog)
  let prefActiveTab: 'general' | 'remote' = 'general';
  let showPrefSearch = false;
  let prefSearchQuery = '';
  let prefDataFolder = '~/.var/app/io.github.redstone/data/redstone';
  let prefRunInBackground = false;
  let prefOpenOnStartup = false;
  let prefPreventSleep = true;
  let prefLanguage = 'System default';
  let prefAutoBackupOnStop = true;
  let showBackupScopeOptions = false;
  let prefBackupScope: 'worlds' | 'full' = 'worlds';
  let prefDeleteBackupsOlderThan = '30 days';
  let prefMaxBackupsPerServer = 0;

  function browseDataFolder() {
    window.alert(`Redstone local data directory:\n${prefDataFolder}`);
  }

  // Catalog State
  let catalogProjectType: ProjectType = 'mod';
  let catalogEnvironment: EnvironmentFilter = 'all';
  let isCategoryCollapsed = false;

  // 19 Authentic Categories matching user screenshot
  const authenticCategories = [
    { id: 'all', label: 'All Categories' },
    { id: 'Adventure', label: 'Adventure' },
    { id: 'Cursed', label: 'Cursed' },
    { id: 'Decoration', label: 'Decoration' },
    { id: 'Economy', label: 'Economy' },
    { id: 'Equipment', label: 'Equipment' },
    { id: 'Food', label: 'Food' },
    { id: 'Game Mechanics', label: 'Game Mechanics' },
    { id: 'Library', label: 'Library' },
    { id: 'Magic', label: 'Magic' },
    { id: 'Management', label: 'Management' },
    { id: 'Minigame', label: 'Minigame' },
    { id: 'Mobs', label: 'Mobs' },
    { id: 'Optimization', label: 'Optimization' },
    { id: 'Social', label: 'Social' },
    { id: 'Storage', label: 'Storage' },
    { id: 'Technology', label: 'Technology' },
    { id: 'Transportation', label: 'Transportation' },
    { id: 'Utility', label: 'Utility' },
    { id: 'World Generation', label: 'World Generation' }
  ];

  const categorySvgPaths: Record<string, string> = {
    all: '<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>',
    Adventure: '<circle cx="12" cy="12" r="10"/><polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76"/>',
    Cursed: '<rect width="8" height="14" x="8" y="6" rx="4"/><path d="m19 7-3 2"/><path d="m5 7 3 2"/><path d="m19 19-3-2"/><path d="m5 19 3-2"/><path d="M20 13h-4"/><path d="M4 13h4"/><path d="m10 4 1 2"/><path d="m14 4-1 2"/>',
    Decoration: '<path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/>',
    Economy: '<line x1="12" x2="12" y1="2" y2="22"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/>',
    Equipment: '<polyline points="14.5 17.5 3 6 3 3 6 3 17.5 14.5"/><line x1="13" x2="19" y1="19" y2="13"/><line x1="16" x2="20" y1="16" y2="20"/><polyline points="14.5 6.5 18 3 21 3 21 6 17.5 9.5"/>',
    Food: '<path d="M12 2a5 5 0 0 0-5 5v14a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V7a5 5 0 0 0-5-5Z"/><path d="M12 2v4"/>',
    'Game Mechanics': '<line x1="4" x2="4" y1="21" y2="14"/><line x1="4" x2="4" y1="10" y2="3"/><line x1="12" x2="12" y1="21" y2="12"/><line x1="12" x2="12" y1="8" y2="3"/><line x1="20" x2="20" y1="21" y2="16"/><line x1="20" x2="20" y1="12" y2="3"/><line x1="1" x2="7" y1="14" y2="14"/><line x1="9" x2="15" y1="8" y2="8"/><line x1="17" x2="23" y1="16" y2="16"/>',
    Library: '<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1-2.5-2.5Z"/><path d="M6 6h10"/><path d="M6 10h10"/>',
    Magic: '<path d="m15 4-2 4 4-2-2-2Z"/><path d="m19 12-1 2 2-1-1-1Z"/><path d="M4 20l10-10"/>',
    Management: '<rect width="20" height="8" x="2" y="3" rx="2"/><rect width="20" height="8" x="2" y="13" rx="2"/><line x1="6" x2="6.01" y1="7" y2="7"/><line x1="6" x2="6.01" y1="17" y2="17"/>',
    Minigame: '<path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6"/><path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18"/><path d="M4 22h16"/><path d="M18 4H6v7a6 6 0 0 0 12 0V4Z"/>',
    Mobs: '<rect width="18" height="18" x="3" y="3" rx="3"/><rect width="3" height="3" x="7" y="7" fill="currentColor"/><rect width="3" height="3" x="14" y="7" fill="currentColor"/><path d="M9 13v3h6v-3"/>',
    Optimization: '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>',
    Social: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
    Storage: '<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>',
    Technology: '<rect width="16" height="16" x="4" y="4" rx="2"/><rect width="6" height="6" x="9" y="9"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/>',
    Transportation: '<rect width="16" height="13" x="1" y="5" rx="2"/><polygon points="16 8 20 8 23 11 23 16 16 16 16 8"/><circle cx="5.5" cy="18.5" r="2.5"/><circle cx="18.5" cy="18.5" r="2.5"/>',
    Utility: '<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>',
    'World Generation': '<circle cx="12" cy="12" r="10"/><line x1="2" x2="22" y1="12" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>'
  };

  const modrinthCategories = authenticCategories;
  const curseforgeCategories = authenticCategories;

  const pluginSupportedLoaders = ['paper', 'spigot', 'bukkit', 'folia', 'purpur', 'sponge'];
  $: effectiveLoader = (instances.find((i) => i.name === settings?.instance_name)?.loader ?? loader).toLowerCase().trim();
  $: supportsPlugins = pluginSupportedLoaders.includes(effectiveLoader);
  $: if (!supportsPlugins && catalogProjectType === 'plugin') {
    catalogProjectType = 'mod';
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      showOptionsDropdown = false;
      showPreferencesModal = false;
      showShortcutsModal = false;
      showAboutModal = false;
    } else if ((e.ctrlKey || e.metaKey) && e.key === ',') {
      e.preventDefault();
      showPreferencesModal = true;
      showOptionsDropdown = false;
    } else if ((e.ctrlKey || e.metaKey) && (e.key === '?' || e.key === '/')) {
      if (document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA') {
        e.preventDefault();
        showShortcutsModal = true;
        showOptionsDropdown = false;
      }
    }
  }

  interface TelemetryPoint {
    tps: number;
    cpu: number;
    ram: number;
    heap: number;
    heapMax: number;
  }
  let telemetryHistory: TelemetryPoint[] = [];

  function computeSparkline(
    values: number[],
    minVal: number,
    maxVal: number,
    width = 280,
    height = 65
  ) {
    if (!values || values.length === 0) {
      return {
        linePath: `M 0,${height / 2} L ${width},${height / 2}`,
        areaPath: `M 0,${height / 2} L ${width},${height / 2} L ${width},${height} L 0,${height} Z`,
        lastX: width,
        lastY: height / 2,
        min: 0,
        max: 0,
        avg: 0
      };
    }

    const effectiveMin = minVal;
    const effectiveMax = Math.max(maxVal, ...values, effectiveMin + 0.001);
    const range = effectiveMax - effectiveMin;
    const len = values.length;
    const step = len > 1 ? width / (len - 1) : width;

    const points = values.map((val, idx) => {
      const x = Number((idx * step).toFixed(1));
      const clamped = Math.max(effectiveMin, Math.min(effectiveMax, val));
      const y = Number((height - 4 - ((clamped - effectiveMin) / range) * (height - 8)).toFixed(1));
      return { x, y };
    });

    const linePath = points.reduce((acc, pt, i) => (i === 0 ? `M ${pt.x},${pt.y}` : `${acc} L ${pt.x},${pt.y}`), '');
    const first = points[0];
    const last = points[points.length - 1];
    const areaPath = `${linePath} L ${last.x},${height} L ${first.x},${height} Z`;

    const min = Math.min(...values);
    const max = Math.max(...values);
    const sum = values.reduce((a, b) => a + b, 0);
    const avg = Number((sum / len).toFixed(1));

    return {
      linePath,
      areaPath,
      lastX: last.x,
      lastY: last.y,
      min,
      max,
      avg
    };
  }

  function formatCompactNumber(num: number): string {
    if (num >= 1_000_000_000) return (num / 1_000_000_000).toFixed(1) + 'B';
    if (num >= 1_000_000) return (num / 1_000_000).toFixed(1) + 'M';
    if (num >= 1_000) return (num / 1_000).toFixed(1) + 'K';
    return num.toLocaleString();
  }

  $: tpsSpark = computeSparkline(telemetryHistory.map(p => p.tps), 0, 20, 280, 65);
  $: cpuSpark = computeSparkline(telemetryHistory.map(p => p.cpu), 0, 100, 280, 65);
  $: ramSpark = computeSparkline(telemetryHistory.map(p => p.ram), 0, Math.max(jvmSettings.ramMb, 4096), 280, 65);
  $: heapSpark = computeSparkline(telemetryHistory.map(p => p.heap), 0, Math.max(resourceMetrics?.heap_max_mb ?? jvmSettings.ramMb, 1024), 280, 65);
  let modResults: ModSearchResult[] = [];
  let modSearchLoading = false;
  let modSearchError = '';
  let modSearchPreview = false;
  let modSearchRevision = 0;
  let installingProjectId = '';
  let installedProjectFiles: Record<string, string> = {};
  let modInstallMessage = '';
  let modInstallError = '';
  let installedMods: InstalledMod[] = [];
  let modUpdates: ModUpdate[] = [];
  let installedModsPreview = false;
  let installedModsLoading = false;
  let updateBusy = '';
  let modManagementBusy = '';
  let modManagementError = '';
  let serverState: ServerStatus = 'stopped';
  let serverBusy = false;
  let serverCommand = '';
  let serverError = '';
  let setupStatus: ServerSetupStatus | null = null;
  let provisionBusy = false;
  let provisionMessage = '';
  let provisionError = '';
  let eulaConfirmedCheckbox = false;
  let instanceActionBusy = false;
  let consoleEvents: ConsoleEvent[] = [];
  let consolePoller: ReturnType<typeof setInterval> | undefined;
  let worldProperties: Record<string, string> = {};
  let customPropertyKey = '';
  let customPropertyValue = '';
  let worldSaveBusy = false;
  let worldMessage = '';
  let jvmSettings: JvmSettings = { ramMb: 4096, preset: 'Custom', custom: '' };
  let jvmSaveBusy = false;
  let jvmMessage = '';
  let memoryMb = systemMemoryMb();
  let resourceMetrics: ResourceMetrics | null = null;
  let metricsPoller: ReturnType<typeof setInterval> | undefined;

  let appMode: 'desktop' | 'mobile' =
    typeof window !== 'undefined' && window.location.search.includes('mode=mobile')
      ? 'mobile'
      : 'desktop';
  let pairingTicket: PairingTicket | null = null;
  let pairedDevices: string[] = [];
  let remoteLoading = false;
  let remoteMessage = '';
  let remoteError = '';
  let mobilePairInput = '';
  let mobilePaired = true;
  let mobileNavTab: 'server' | 'console' | 'players' = 'server';

  const standardPropKeys = new Set([
    'level-name',
    'level-seed',
    'motd',
    'difficulty',
    'gamemode',
    'hardcore',
    'pvp',
    'online-mode',
    'white-list',
    'allow-flight',
    'enable-command-block',
    'force-gamemode',
    'spawn-monsters',
    'spawn-animals',
    'spawn-npcs',
    'generate-structures',
    'max-players',
    'view-distance',
    'simulation-distance',
    'server-port'
  ]);

  $: customPropEntries = Object.entries(worldProperties).filter(([k]) => !standardPropKeys.has(k));

  function getProp(key: string, def = ''): string {
    return worldProperties[key] ?? def;
  }

  function setProp(key: string, val: string) {
    worldProperties = { ...worldProperties, [key]: val };
  }

  function toggleProp(key: string, def = 'false') {
    const current = getProp(key, def) === 'true';
    setProp(key, (!current).toString());
  }

  function isPropTrue(key: string, def = 'false'): boolean {
    return getProp(key, def) === 'true';
  }

  function removeCustomProp(key: string) {
    const next = { ...worldProperties };
    delete next[key];
    worldProperties = next;
  }

  function addCustomProp() {
    const k = customPropertyKey.trim();
    if (!k) return;
    setProp(k, customPropertyValue.trim());
    customPropertyKey = '';
    customPropertyValue = '';
  }

  let currentFilePath = '';
  let fileEntries: FileEntry[] = [];
  let filesLoading = false;
  let filesError = '';
  let selectedFile: FileEntry | null = null;
  let selectedFileContent = '';
  let editingFileContent = '';
  let fileSaveBusy = false;
  let fileMessage = '';

  let playerLists: PlayerLists = { whitelist: [], banlist: [] };
  let playersLoading = false;
  let playersError = '';
  let playersMessage = '';
  let newWhitelistName = '';
  let newBanName = '';

  let unlistenConsole: (() => void) | undefined;
  let unlistenState: (() => void) | undefined;

  let joinInfo: JoinInfo | null = null;
  let copiedAddress = false;
  let copyTimeout: ReturnType<typeof setTimeout> | undefined;
  let showQrModal = false;
  let dnsSecretToken = '';
  let dnsSecretMasked = true;
  let dnsSecretBusy = false;
  let dnsZoneId = '';
  let dnsSyncBusy = false;
  let dnsSyncMessage = '';
  let dnsSyncError = '';

  const modSearchDebouncer = createDebouncer(() => {
    void runModSearch();
  }, 250);

  onDestroy(() => {
    modSearchDebouncer.cancel();
    unlistenConsole?.();
    unlistenState?.();
    if (consolePoller) clearInterval(consolePoller);
    if (metricsPoller) clearInterval(metricsPoller);
    if (copyTimeout) clearTimeout(copyTimeout);
  });

  function handleTabKeydown(event: KeyboardEvent, tab: WorkspaceTab) {
    let nextIndex = workspaceTabs.indexOf(tab);
    if (event.key === 'ArrowRight') nextIndex = (nextIndex + 1) % workspaceTabs.length;
    else if (event.key === 'ArrowLeft') nextIndex = (nextIndex - 1 + workspaceTabs.length) % workspaceTabs.length;
    else if (event.key === 'Home') nextIndex = 0;
    else if (event.key === 'End') nextIndex = workspaceTabs.length - 1;
    else return;

    event.preventDefault();
    activeTab = workspaceTabs[nextIndex];
    document.getElementById(`tab-${activeTab}`)?.focus();
  }

  async function initializeSettings() {
    try {
      settings = await loadSettings();
      instanceName = settings.instance_name ? `${settings.instance_name} copy` : instanceName;
      instances = await listInstances();
      if (!isTauri() && instances.length === 0 && typeof window !== 'undefined' && new URLSearchParams(window.location.search).has('sample')) {
        const sample: CreatedInstance = {
          id: 1,
          name: 'Survival',
          minecraft_version: '1.21.4',
          loader: 'neoforge',
          previewOnly: true
        };
        instances = [sample];
        settings = { ...settings, instance_name: 'Survival' };
        try {
          localStorage.setItem('redstone.preview.instances', JSON.stringify([sample]));
          localStorage.setItem('redstone.settings', JSON.stringify({ ...settings, instance_name: 'Survival' }));
        } catch {}
      }
      const selectedInstance = instances.find((instance) => instance.name === settings?.instance_name);
      if (selectedInstance) {
        minecraftVersion = selectedInstance.minecraft_version;
        loader = selectedInstance.loader;
      }
      if (settings.instance_name) await refreshInstalledMods(settings.instance_name);
      if (settings.instance_name) await refreshServerStatus(settings.instance_name);
      if (settings.instance_name) await refreshSetupStatus(settings.instance_name);
      if (settings.instance_name) await refreshWorldProperties(settings.instance_name);
      if (settings.instance_name) await refreshJvmSettings(settings.instance_name);
      if (settings.instance_name) await loadFiles(settings.instance_name, '');
      if (settings.instance_name) await loadPlayers(settings.instance_name);
      if (settings.instance_name) await attachServerListeners(settings.instance_name);
      await refreshJoinInfo(settings.instance_name ?? undefined);
      if (settings.dns_provider) await loadDnsToken(settings.dns_provider);
      await loadCurseForgeKey();
      await refreshPairedDevices();
      startMetricsPolling(settings.instance_name ?? undefined);
      startConsolePolling(settings.instance_name ?? undefined);
    } catch (loadError: unknown) {
      error = loadError instanceof Error ? loadError.message : String(loadError);
    } finally {
      loading = false;
    }
  }

  void initializeSettings();

  function setLanOnly(lanOnly: boolean) {
    if (settings) {
      settings = {
        ...settings,
        lan_only: lanOnly,
        dns_provider: lanOnly ? settings.dns_provider : settings.dns_provider ?? 'manual',
        dns_subdomain: lanOnly ? settings.dns_subdomain : settings.dns_subdomain ?? 'play'
      };
      saved = false;
      void refreshJoinInfo();
    }
  }

  function updateDnsSettings(field: 'dns_provider' | 'dns_subdomain' | 'dns_domain', value: string) {
    if (settings) {
      settings = { ...settings, [field]: value.trim() || null };
      saved = false;
      void refreshJoinInfo();
    }
  }

  function tokenKeyForProvider(provider: string): string {
    if (provider === 'duckdns') return 'duckdns_token';
    if (provider === 'desec') return 'desec_token';
    if (provider === 'playit') return 'playit_token';
    return 'cloudflare_token';
  }

  async function refreshJoinInfo(targetName = settings?.instance_name ?? createdInstance?.name ?? 'default') {
    try {
      joinInfo = await getJoinInfo(targetName);
    } catch {
      // ignore
    }
  }

  async function loadDnsToken(provider: string) {
    if (provider === 'manual') {
      dnsSecretToken = '';
      return;
    }
    try {
      const token = await getNetworkSecret(tokenKeyForProvider(provider));
      dnsSecretToken = token ?? '';
    } catch {
      dnsSecretToken = '';
    }
  }

  async function loadCurseForgeKey() {
    try {
      const key = await getCurseForgeApiKey();
      curseforgeApiKey = key ?? '';
      curseforgeApiKeySaved = Boolean(key && key.trim().length > 0);
    } catch {
      curseforgeApiKey = '';
      curseforgeApiKeySaved = false;
    }
  }

  async function saveCurseForgeKey() {
    if (!curseforgeApiKey.trim()) return;
    curseforgeKeyBusy = true;
    curseforgeKeyMessage = '';
    curseforgeKeyError = '';
    try {
      await saveCurseForgeApiKey(curseforgeApiKey.trim());
      curseforgeApiKeySaved = true;
      curseforgeKeyMessage = 'CurseForge API key saved securely in OS keychain.';
      scheduleModSearch();
    } catch (e: unknown) {
      curseforgeKeyError = e instanceof Error ? e.message : String(e);
    } finally {
      curseforgeKeyBusy = false;
    }
  }

  function onDnsProviderChange(provider: string) {
    updateDnsSettings('dns_provider', provider);
    if (provider === 'duckdns') {
      updateDnsSettings('dns_domain', 'duckdns.org');
    } else if (provider === 'desec') {
      updateDnsSettings('dns_domain', 'dedyn.io');
    } else if (provider === 'playit') {
      updateDnsSettings('dns_domain', 'gl.joinmc.link');
    }
    void loadDnsToken(provider);
  }

  async function copyJoinAddress() {
    if (!joinInfo) return;
    const addr = joinInfo.qr_payload;
    try {
      await navigator.clipboard.writeText(addr);
      copiedAddress = true;
      if (copyTimeout) clearTimeout(copyTimeout);
      copyTimeout = setTimeout(() => {
        copiedAddress = false;
      }, 2000);
    } catch {
      // fallback
    }
  }

  async function saveDnsSecret() {
    const provider = settings?.dns_provider ?? 'manual';
    if (provider === 'manual' || !dnsSecretToken.trim()) return;
    dnsSecretBusy = true;
    dnsSyncMessage = '';
    dnsSyncError = '';
    try {
      await saveNetworkSecret(tokenKeyForProvider(provider), dnsSecretToken.trim());
      dnsSyncMessage = 'Token saved securely to OS keychain.';
    } catch (err: unknown) {
      dnsSyncError = err instanceof Error ? err.message : String(err);
    } finally {
      dnsSecretBusy = false;
    }
  }

  async function syncDns() {
    const provider = settings?.dns_provider ?? 'manual';
    if (provider === 'manual') return;
    const domain = provider === 'duckdns' || provider === 'playit'
      ? (settings?.dns_subdomain ?? 'play')
      : `${settings?.dns_subdomain ?? 'play'}.${settings?.dns_domain ?? ''}`;
    dnsSyncBusy = true;
    dnsSyncMessage = '';
    dnsSyncError = '';
    try {
      const res = await updateDnsRecords({
        provider,
        domain,
        tokenKey: tokenKeyForProvider(provider),
        zoneId: provider === 'cloudflare' ? dnsZoneId.trim() : undefined
      });
      dnsSyncMessage = res;
    } catch (err: unknown) {
      dnsSyncError = err instanceof Error ? err.message : String(err);
    } finally {
      dnsSyncBusy = false;
    }
  }

  async function refreshPairedDevices() {
    try {
      pairedDevices = await listPairedDevices();
    } catch {
      // preview fallback
    }
  }

  async function generateTicket() {
    remoteLoading = true;
    remoteError = '';
    remoteMessage = '';
    try {
      pairingTicket = await getRemotePairingTicket();
      remoteMessage = 'Pairing ticket generated. Scan QR code or enter code on your mobile companion.';
    } catch (e: unknown) {
      remoteError = e instanceof Error ? e.message : String(e);
    } finally {
      remoteLoading = false;
    }
  }

  async function revokeDevice(deviceId: string) {
    if (!confirm(`Revoke remote access for device "${deviceId}"?`)) return;
    remoteLoading = true;
    remoteError = '';
    remoteMessage = '';
    try {
      await revokePairedDevice(deviceId);
      await refreshPairedDevices();
      remoteMessage = `Device "${deviceId}" access revoked.`;
    } catch (e: unknown) {
      remoteError = e instanceof Error ? e.message : String(e);
    } finally {
      remoteLoading = false;
    }
  }

  async function submitMobilePair() {
    if (!mobilePairInput.trim()) return;
    remoteLoading = true;
    remoteError = '';
    remoteMessage = '';
    try {
      let code = mobilePairInput.trim();
      let endpoint = 'redstone-preview-host';
      if (code.startsWith('redstone://pair/')) {
        const parts = code.replace('redstone://pair/', '').split('/');
        endpoint = parts[0] || endpoint;
        code = parts[1] || code;
      }
      await pairRemoteDevice('Mobile-Companion', code);
      mobilePaired = true;
      remoteMessage = 'Paired successfully with Redstone Host!';
      await refreshPairedDevices();
    } catch (e: unknown) {
      remoteError = e instanceof Error ? e.message : String(e);
    } finally {
      remoteLoading = false;
    }
  }

  function scheduleModSearch() {
    modSearchRevision += 1;
    modResults = [];
    modSearchError = '';
    if (modQuery.trim().length < 2 && modCatalogCategory === 'all' && catalogProjectType === 'mod') {
      modSearchDebouncer.cancel();
      modSearchLoading = false;
      return;
    }
    modSearchDebouncer.schedule();
  }

  async function refreshInstalledMods(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) {
      installedMods = [];
      modUpdates = [];
      return;
    }
    installedModsLoading = true;
    modManagementError = '';
    try {
      const response = await listInstalledMods(targetName);
      installedMods = response.mods;
      installedModsPreview = response.previewOnly;
      const selectedInstance = instances.find((instance) => instance.name === targetName);
      modUpdates = await checkModUpdates(
        targetName,
        selectedInstance?.minecraft_version ?? minecraftVersion,
        selectedInstance?.loader ?? loader
      );
    } catch (listError: unknown) {
      modManagementError = listError instanceof Error ? listError.message : String(listError);
    } finally {
      installedModsLoading = false;
    }
  }

  async function toggleInstalledMod(mod: InstalledMod) {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || modManagementBusy) return;
    modManagementBusy = mod.file_name;
    modManagementError = '';
    modInstallMessage = '';
    try {
      await setModEnabled(targetName, mod.file_name, !mod.enabled);
      await refreshInstalledMods(targetName);
    } catch (toggleError: unknown) {
      modManagementError = toggleError instanceof Error ? toggleError.message : String(toggleError);
    } finally {
      modManagementBusy = '';
    }
  }

  async function removeInstalledMod(mod: InstalledMod) {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || modManagementBusy) return;
    if (!window.confirm(`Remove ${mod.file_name} from ${targetName}? This cannot be undone.`)) return;

    modManagementBusy = mod.file_name;
    modManagementError = '';
    modInstallMessage = '';
    try {
      await removeMod(targetName, mod.file_name, true);
      installedProjectFiles = Object.entries(installedProjectFiles).reduce(
        (remaining, [projectId, fileName]) => {
          if (fileName !== mod.file_name) remaining[projectId] = fileName;
          return remaining;
        },
        {} as Record<string, string>
      );
      await refreshInstalledMods(targetName);
    } catch (removeError: unknown) {
      modManagementError = removeError instanceof Error ? removeError.message : String(removeError);
    } finally {
      modManagementBusy = '';
    }
  }

  async function applyAvailableUpdate(update: ModUpdate) {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || updateBusy) return;
    const confirmed = window.confirm(
      `Update ${update.file_name} to ${update.latest_version_number}? The current JAR will be replaced.`
    );
    if (!confirmed) return;

    updateBusy = update.project_id;
    modManagementError = '';
    try {
      const selectedInstance = instances.find((instance) => instance.name === targetName);
      const result = await applyModUpdate(
        targetName,
        update,
        selectedInstance?.minecraft_version ?? minecraftVersion,
        selectedInstance?.loader ?? loader,
        true
      );
      installedProjectFiles = { ...installedProjectFiles, [result.project_id]: result.file_name };
      await refreshInstalledMods(targetName);
    } catch (updateError: unknown) {
      modManagementError = updateError instanceof Error ? updateError.message : String(updateError);
    } finally {
      updateBusy = '';
    }
  }

  async function runModSearch() {
    if (!settings) return;
    const revision = modSearchRevision;
    modSearchLoading = true;
    modSearchError = '';
    try {
      const selectedInstance = instances.find((instance) => instance.name === settings?.instance_name);
      const activeVersion = selectedInstance?.minecraft_version ?? minecraftVersion;
      const activeLoader = selectedInstance?.loader ?? loader;
      const response = await searchMods(
        modQuery.trim(),
        activeVersion,
        activeLoader,
        modCatalogSource,
        modCatalogCategory,
        catalogProjectType,
        catalogEnvironment
      );
      if (revision !== modSearchRevision) return;
      modResults = response.results;
      modSearchPreview = response.previewOnly;
    } catch (searchError: unknown) {
      if (revision !== modSearchRevision) return;
      modSearchError = searchError instanceof Error ? searchError.message : String(searchError);
    } finally {
      if (revision === modSearchRevision) modSearchLoading = false;
    }
  }

  async function installSelectedMod(mod: ModSearchResult) {
    if (!settings || installingProjectId) return;
    const confirmed = window.confirm(
      `Install ${mod.title} for Minecraft ${minecraftVersion} (${loader})? This will not replace an existing mod file.`
    );
    if (!confirmed) return;

    installingProjectId = mod.project_id;
    modInstallMessage = '';
    modInstallError = '';
    try {
      if (!settings.instance_name && !createdInstance) {
        createdInstance = await createInstance({
          name: instanceName,
          minecraftVersion,
          loader
        });
        settings = { ...settings, instance_name: createdInstance.name };
        await saveSettings(settings);
        instances = await listInstances();
      }

      const targetInstance = settings.instance_name ?? createdInstance?.name;
      if (!targetInstance) throw new Error('Create an instance before installing a mod.');

      const result = await installMod(targetInstance, mod, minecraftVersion, loader);
      installedProjectFiles = { ...installedProjectFiles, [mod.project_id]: result.fileName };
      await refreshInstalledMods(targetInstance);
      modInstallMessage = result.previewOnly
        ? `${result.fileName} is a preview only; no file was downloaded in the browser.`
        : `Installed ${result.fileName} in ${targetInstance}/mods.`;
    } catch (installError: unknown) {
      modInstallError = installError instanceof Error ? installError.message : String(installError);
    } finally {
      installingProjectId = '';
    }
  }

  async function submitSettings(event: SubmitEvent) {
    event.preventDefault();
    if (!settings) return;

    saving = true;
    saved = false;
    error = '';

    try {
      const updatedSettings = { ...settings, onboarding_complete: true };
      await saveSettings(updatedSettings);
      settings = updatedSettings;
      await refreshInstalledMods(updatedSettings.instance_name ?? undefined);
      await refreshJoinInfo(updatedSettings.instance_name ?? undefined);
      saved = true;
    } catch (saveError: unknown) {
      error = saveError instanceof Error ? saveError.message : String(saveError);
    } finally {
      saving = false;
    }
  }

  async function createNewInstance() {
    if (!settings || saving) return;
    saving = true;
    error = '';
    try {
      const instance = await createInstance({
        name: instanceName,
        minecraftVersion,
        loader
      });
      createdInstance = instance;
      instances = await listInstances();
      settings = { ...settings, instance_name: instance.name };
      eulaConfirmedCheckbox = false;
      provisionMessage = '';
      provisionError = '';
      await saveSettings(settings);
      await refreshInstalledMods(instance.name);
      await refreshSetupStatus(instance.name);
      await refreshJoinInfo(instance.name);
      instanceName = `${instance.name} copy`;
    } catch (createError: unknown) {
      error = createError instanceof Error ? createError.message : String(createError);
    } finally {
      saving = false;
    }
  }

  function uniqueCopyName(baseName: string): string {
    const existing = new Set(instances.map((item) => item.name));
    const trimmedBase = baseName.slice(0, 38).trim();
    const firstCandidate = `${trimmedBase} copy`;
    if (!existing.has(firstCandidate)) return firstCandidate;
    for (let index = 2; index < 100; index += 1) {
      const candidate = `${trimmedBase} copy ${index}`;
      if (!existing.has(candidate)) return candidate;
    }
    return `${trimmedBase.slice(0, 32)} ${Date.now() % 10000}`;
  }

  async function duplicateSelectedInstance() {
    const sourceName = settings?.instance_name;
    if (!settings || !sourceName || instanceActionBusy) return;
    instanceActionBusy = true;
    error = '';
    try {
      const copy = await duplicateInstance(sourceName, uniqueCopyName(sourceName));
      instances = await listInstances();
      await selectInstance(copy);
    } catch (dupError: unknown) {
      error = dupError instanceof Error ? dupError.message : String(dupError);
    } finally {
      instanceActionBusy = false;
    }
  }

  async function deleteSelectedInstance() {
    const targetName = settings?.instance_name;
    if (!settings || !targetName || instanceActionBusy) return;
    if (serverState === 'running') {
      error = 'Stop the running server before deleting this instance.';
      return;
    }
    const confirmed = window.confirm(
      `Delete instance "${targetName}" and all its files? This cannot be undone.`
    );
    if (!confirmed) return;

    instanceActionBusy = true;
    error = '';
    try {
      await deleteInstance(targetName, true);
      instances = await listInstances();
      if (instances.length > 0) {
        await selectInstance(instances[0]);
      } else {
        createdInstance = null;
        setupStatus = null;
        eulaConfirmedCheckbox = false;
        settings = { ...settings, instance_name: null };
        await saveSettings(settings);
        await refreshInstalledMods(undefined);
        await refreshServerStatus(undefined);
        await refreshWorldProperties(undefined);
      }
    } catch (delError: unknown) {
      error = delError instanceof Error ? delError.message : String(delError);
    } finally {
      instanceActionBusy = false;
    }
  }

  async function selectInstance(instance: CreatedInstance) {
    if (!settings) return;
    settings = { ...settings, instance_name: instance.name };
    minecraftVersion = instance.minecraft_version;
    loader = instance.loader;
    createdInstance = instance.previewOnly ? instance : null;
    eulaConfirmedCheckbox = false;
    provisionMessage = '';
    provisionError = '';
    saved = false;
    try {
      await saveSettings({ ...settings, onboarding_complete: true });
      await refreshInstalledMods(instance.name);
      await refreshServerStatus(instance.name);
      await refreshSetupStatus(instance.name);
      await refreshWorldProperties(instance.name);
      await refreshJvmSettings(instance.name);
      await loadFiles(instance.name, '');
      await loadPlayers(instance.name);
      await attachServerListeners(instance.name);
      await refreshJoinInfo(instance.name);
      if (settings.dns_provider) await loadDnsToken(settings.dns_provider);
      startMetricsPolling(instance.name);
      startConsolePolling(instance.name);
    } catch (selectError: unknown) {
      error = selectError instanceof Error ? selectError.message : String(selectError);
    }
  }

  async function refreshSetupStatus(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) {
      setupStatus = null;
      return;
    }
    try {
      setupStatus = await serverSetupStatus(targetName);
    } catch (statusErr: unknown) {
      provisionError = statusErr instanceof Error ? statusErr.message : String(statusErr);
    }
  }

  async function provisionServerJar() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || provisionBusy) return;
    provisionBusy = true;
    provisionMessage = '';
    provisionError = '';
    try {
      const result = await downloadMojangServerJar(targetName, minecraftVersion);
      await refreshSetupStatus(targetName);
      provisionMessage = result.previewOnly
        ? `Provisioned preview server.jar for Minecraft ${result.minecraft_version}.`
        : `Downloaded official Mojang server.jar (${Math.max(1, Math.round(result.jar_size_bytes / (1024 * 1024)))} MiB) for Minecraft ${result.minecraft_version}.`;
    } catch (dlError: unknown) {
      provisionError = dlError instanceof Error ? dlError.message : String(dlError);
    } finally {
      provisionBusy = false;
    }
  }

  async function confirmMojangEula() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || !eulaConfirmedCheckbox || provisionBusy) return;
    provisionBusy = true;
    provisionMessage = '';
    provisionError = '';
    try {
      await acceptMojangEula(targetName, true);
      await refreshSetupStatus(targetName);
      provisionMessage = 'Mojang EULA accepted (eula=true written to eula.txt).';
    } catch (eulaError: unknown) {
      provisionError = eulaError instanceof Error ? eulaError.message : String(eulaError);
    } finally {
      provisionBusy = false;
    }
  }

  async function refreshServerStatus(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) {
      serverState = 'stopped';
      return;
    }
    try {
      serverState = await serverStatus(targetName);
      serverError = '';
    } catch (statusError: unknown) {
      serverError = statusError instanceof Error ? statusError.message : String(statusError);
    }
  }

  function startConsolePolling(targetName: string | undefined) {
    if (consolePoller) clearInterval(consolePoller);
    consoleEvents = [];
    if (!targetName) return;
    consolePoller = setInterval(() => {
      void pollServerConsole(targetName);
    }, 1000);
  }

  async function pollServerConsole(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) return;
    try {
      const events = await readServerConsole(targetName);
      if (events.length > 0) consoleEvents = [...consoleEvents, ...events].slice(-200);
    } catch (consoleError: unknown) {
      serverError = consoleError instanceof Error ? consoleError.message : String(consoleError);
    }
  }

  async function toggleServer() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || serverBusy) return;
    serverBusy = true;
    serverError = '';
    try {
      serverState = serverState === 'running'
        ? await stopServer(targetName)
        : await startServer(targetName);
      await pollServerConsole(targetName);
    } catch (lifecycleError: unknown) {
      serverError = lifecycleError instanceof Error ? lifecycleError.message : String(lifecycleError);
      await refreshServerStatus(targetName);
    } finally {
      await refreshSetupStatus(targetName);
      serverBusy = false;
    }
  }

  async function submitServerCommand() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    const command = serverCommand.trim();
    if (!targetName || !command || serverBusy) return;
    serverBusy = true;
    serverError = '';
    try {
      await sendServerCommand(targetName, command);
      serverCommand = '';
    } catch (commandError: unknown) {
      serverError = commandError instanceof Error ? commandError.message : String(commandError);
    } finally {
      await refreshSetupStatus(targetName);
      serverBusy = false;
    }
  }

  async function refreshWorldProperties(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) {
      worldProperties = {};
      return;
    }
    try {
      worldProperties = await readWorldProperties(targetName);
      worldMessage = '';
    } catch (worldError: unknown) {
      worldMessage = worldError instanceof Error ? worldError.message : String(worldError);
    }
  }

  async function saveWorldProperties() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || worldSaveBusy) return;
    worldSaveBusy = true;
    worldMessage = '';
    try {
      await writeWorldProperties(targetName, worldProperties);
      worldMessage = 'server.properties saved.';
    } catch (saveError: unknown) {
      worldMessage = saveError instanceof Error ? saveError.message : String(saveError);
    } finally {
      worldSaveBusy = false;
    }
  }

  async function refreshJvmSettings(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) return;
    try {
      jvmSettings = await readJvmSettings(targetName);
      jvmMessage = '';
    } catch (jvmError: unknown) {
      jvmMessage = jvmError instanceof Error ? jvmError.message : String(jvmError);
    }
  }

  async function saveJvmSettings() {
    const targetName = settings?.instance_name ?? createdInstance?.name;
    if (!targetName || jvmSaveBusy) return;
    jvmSaveBusy = true;
    jvmMessage = '';
    try {
      await writeJvmSettings(targetName, jvmSettings);
      jvmMessage = 'JVM settings saved for the next start.';
    } catch (saveError: unknown) {
      jvmMessage = saveError instanceof Error ? saveError.message : String(saveError);
    } finally {
      jvmSaveBusy = false;
    }
  }

  function startMetricsPolling(targetName: string | undefined) {
    if (metricsPoller) clearInterval(metricsPoller);
    resourceMetrics = null;
    if (!targetName) return;
    metricsPoller = setInterval(() => {
      void refreshResourceMetrics(targetName);
    }, 1000);
    void refreshResourceMetrics(targetName);
  }

  async function refreshResourceMetrics(targetName = settings?.instance_name ?? createdInstance?.name) {
    if (!targetName) return;
    try {
      resourceMetrics = await readResourceMetrics(targetName);
      if (resourceMetrics) {
        const currentTps = resourceMetrics.tps ?? (serverState === 'running' ? 20.0 : 0);
        const currentCpu = resourceMetrics.cpu_percent ?? 0;
        const currentRam = resourceMetrics.memory_mb ?? 0;
        const currentHeap = resourceMetrics.heap_used_mb ?? 0;
        const currentHeapMax = resourceMetrics.heap_max_mb ?? jvmSettings.ramMb;

        if (telemetryHistory.length === 0) {
          const seeds: TelemetryPoint[] = [];
          for (let i = 15; i >= 1; i--) {
            seeds.push({
              tps: Number((currentTps - (Math.sin(i) * 0.15)).toFixed(2)),
              cpu: Math.max(1, Number((currentCpu - (Math.cos(i) * 2.5)).toFixed(1))),
              ram: Math.max(512, Math.round(currentRam - (i * 2))),
              heap: Math.max(256, Math.round(currentHeap - (i * 15))),
              heapMax: currentHeapMax
            });
          }
          telemetryHistory = seeds;
        }

        const point: TelemetryPoint = {
          tps: currentTps,
          cpu: currentCpu,
          ram: currentRam,
          heap: currentHeap,
          heapMax: currentHeapMax
        };
        telemetryHistory = [...telemetryHistory.slice(-29), point];
      }
    } catch (metricsError: unknown) {
      serverError = metricsError instanceof Error ? metricsError.message : String(metricsError);
    }
  }

  function chooseJvmPreset(preset: JvmSettings['preset']) {
    if (jvmSettings.preset === 'Custom' && jvmSettings.custom.trim() && preset !== 'Custom'
      && !window.confirm('Replace unsaved custom JVM arguments with the Performance preset?')) return;
    jvmSettings = { ...jvmSettings, preset, custom: preset === 'Custom' ? jvmSettings.custom : '' };
  }

  async function attachServerListeners(targetInstance: string) {
    unlistenConsole?.();
    unlistenState?.();
    unlistenConsole = await onServerConsole(targetInstance, (event) => {
      consoleEvents = [...consoleEvents.slice(-199), event];
    });
    unlistenState = await onServerStateChanged(targetInstance, (state) => {
      serverState = state;
    });
  }

  async function loadFiles(targetInstance = settings?.instance_name ?? createdInstance?.name, path = currentFilePath) {
    if (!targetInstance) return;
    filesLoading = true;
    filesError = '';
    fileMessage = '';
    try {
      currentFilePath = path;
      fileEntries = await explorerListDirectory(targetInstance, path);
    } catch (err: unknown) {
      filesError = err instanceof Error ? err.message : String(err);
    } finally {
      filesLoading = false;
    }
  }

  async function openFile(entry: FileEntry) {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    if (!targetInstance) return;
    if (entry.is_dir) {
      await loadFiles(targetInstance, entry.relative_path);
    } else {
      filesLoading = true;
      filesError = '';
      fileMessage = '';
      try {
        const content = await explorerReadFile(targetInstance, entry.relative_path);
        selectedFile = entry;
        selectedFileContent = content;
        editingFileContent = content;
      } catch (err: unknown) {
        filesError = err instanceof Error ? err.message : String(err);
      } finally {
        filesLoading = false;
      }
    }
  }

  async function saveOpenedFile() {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    if (!targetInstance || !selectedFile) return;
    fileSaveBusy = true;
    filesError = '';
    try {
      await explorerWriteFile(targetInstance, selectedFile.relative_path, editingFileContent);
      selectedFileContent = editingFileContent;
      fileMessage = `Saved ${selectedFile.name}`;
      await loadFiles(targetInstance, currentFilePath);
    } catch (err: unknown) {
      filesError = err instanceof Error ? err.message : String(err);
    } finally {
      fileSaveBusy = false;
    }
  }

  async function deleteEntry(entry: FileEntry) {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    if (!targetInstance) return;
    if (!window.confirm(`Are you sure you want to delete ${entry.name}?`)) return;
    filesLoading = true;
    filesError = '';
    try {
      await explorerDeleteFile(targetInstance, entry.relative_path, true);
      fileMessage = `Deleted ${entry.name}`;
      if (selectedFile?.relative_path === entry.relative_path) {
        selectedFile = null;
        selectedFileContent = '';
        editingFileContent = '';
      }
      await loadFiles(targetInstance, currentFilePath);
    } catch (err: unknown) {
      filesError = err instanceof Error ? err.message : String(err);
    } finally {
      filesLoading = false;
    }
  }

  function navigateUp() {
    if (!currentFilePath) return;
    const parts = currentFilePath.split('/');
    parts.pop();
    const parentPath = parts.join('/');
    void loadFiles(settings?.instance_name ?? createdInstance?.name, parentPath);
  }

  async function loadPlayers(targetInstance = settings?.instance_name ?? createdInstance?.name) {
    if (!targetInstance) return;
    playersLoading = true;
    playersError = '';
    try {
      playerLists = await readPlayerLists(targetInstance);
    } catch (err: unknown) {
      playersError = err instanceof Error ? err.message : String(err);
    } finally {
      playersLoading = false;
    }
  }

  async function addPlayerToWhitelist() {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    const name = newWhitelistName.trim();
    if (!targetInstance || !name) return;
    if (playerLists.whitelist.some((p) => p.name.toLowerCase() === name.toLowerCase())) {
      playersError = `${name} is already whitelisted.`;
      return;
    }
    playersLoading = true;
    playersError = '';
    try {
      const updated = [...playerLists.whitelist, { name, uuid: generateOfflineUuid(name) }];
      await writeWhitelist(targetInstance, updated);
      playerLists.whitelist = updated;
      newWhitelistName = '';
      playersMessage = `Added ${name} to whitelist.`;
    } catch (err: unknown) {
      playersError = err instanceof Error ? err.message : String(err);
    } finally {
      playersLoading = false;
    }
  }

  async function removePlayerFromWhitelist(entry: PlayerEntry) {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    if (!targetInstance) return;
    if (!window.confirm(`Remove ${entry.name} from whitelist?`)) return;
    playersLoading = true;
    playersError = '';
    try {
      const updated = playerLists.whitelist.filter((p) => p.name !== entry.name);
      await writeWhitelist(targetInstance, updated);
      playerLists.whitelist = updated;
      playersMessage = `Removed ${entry.name} from whitelist.`;
    } catch (err: unknown) {
      playersError = err instanceof Error ? err.message : String(err);
    } finally {
      playersLoading = false;
    }
  }

  async function addPlayerToBanlist() {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    const name = newBanName.trim();
    if (!targetInstance || !name) return;
    if (playerLists.banlist.some((p) => p.name.toLowerCase() === name.toLowerCase())) {
      playersError = `${name} is already banned.`;
      return;
    }
    playersLoading = true;
    playersError = '';
    try {
      const updated = [...playerLists.banlist, { name, uuid: generateOfflineUuid(name) }];
      await writeBanlist(targetInstance, updated);
      playerLists.banlist = updated;
      newBanName = '';
      playersMessage = `Banned ${name}.`;
    } catch (err: unknown) {
      playersError = err instanceof Error ? err.message : String(err);
    } finally {
      playersLoading = false;
    }
  }

  async function removePlayerFromBanlist(entry: PlayerEntry) {
    const targetInstance = settings?.instance_name ?? createdInstance?.name;
    if (!targetInstance) return;
    if (!window.confirm(`Unban ${entry.name}?`)) return;
    playersLoading = true;
    playersError = '';
    try {
      const updated = playerLists.banlist.filter((p) => p.name !== entry.name);
      await writeBanlist(targetInstance, updated);
      playerLists.banlist = updated;
      playersMessage = `Unbanned ${entry.name}.`;
    } catch (err: unknown) {
      playersError = err instanceof Error ? err.message : String(err);
    } finally {
      playersLoading = false;
    }
  }
</script>

{#if appMode === 'mobile'}
  <main class="mobile-shell" role="region" aria-label="Mobile Companion View">
    <header class="mobile-header">
      <div class="mobile-header-top">
        <div class="mobile-brand">
          <span class="brand-mark" aria-hidden="true">R</span>
          <div class="mobile-brand-text">
            <strong>REDSTONE</strong>
            <small>REMOTE COMPANION</small>
          </div>
        </div>
        <div class="mobile-header-actions">
          <span class="iroh-badge" title="Identity verified via Ed25519">
            <span class="iroh-dot" aria-hidden="true"></span>
            Iroh Encrypted
          </span>
          <button
            type="button"
            class="secondary-button small mobile-exit-btn"
            on:click={() => (appMode = 'desktop')}
            aria-label="Switch to desktop host view"
          >
            Desktop Host ↗
          </button>
        </div>
      </div>

      <div class="mobile-status-strip">
        <div class="mobile-instance-meta">
          <span
            class="server-status-dot"
            class:dot-running={serverState === 'running'}
            class:dot-crashed={serverState === 'crashed'}
            class:dot-stopped={serverState === 'stopped'}
            aria-hidden="true"
          ></span>
          <strong class="mobile-instance-name">{settings?.instance_name ?? 'Survival'}</strong>
          <span class="mobile-ver">{minecraftVersion} · {formatLoader(loader)}</span>
        </div>
        <span
          class="status-pill"
          class:running={serverState === 'running'}
          class:crashed={serverState === 'crashed'}
          aria-label={`Server status: ${serverState}`}
        >
          {#if serverState === 'running'}● RUNNING{:else if serverState === 'crashed'}▲ CRASHED{:else}■ STOPPED{/if}
        </span>
      </div>
    </header>

    {#if !mobilePaired}
      <div class="mobile-content mobile-pairing-gate">
        <div class="mobile-card">
          <h3>Pair with Redstone Host</h3>
          <p class="field-hint">
            Enter the 6-character pairing ticket code displayed on your desktop Redstone host or paste the redstone:// URL.
          </p>
          {#if remoteError}
            <p class="catalog-message error" role="alert">{remoteError}</p>
          {/if}
          <form class="mobile-pair-form" on:submit|preventDefault={() => void submitMobilePair()}>
            <label class="form-field-dark">
              <span>Pairing Code / Ticket URL</span>
              <input
                type="text"
                placeholder="e.g. a1b2c3"
                bind:value={mobilePairInput}
                autocomplete="off"
                required
              />
            </label>
            <button
              type="submit"
              class="save-button mobile-touch-button"
              disabled={remoteLoading || !mobilePairInput.trim()}
            >
              {remoteLoading ? 'Verifying...' : 'Connect to Host'}
            </button>
          </form>
        </div>
      </div>
    {:else}
      <div class="mobile-content">
        {#if serverError}
          <p class="catalog-message error" role="alert">{serverError}</p>
        {/if}
        {#if resourceMetrics?.low_tps_alert}
          <div class="low-tps-alert-banner mobile-alert" role="alert">
            <span class="alert-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
                <line x1="12" y1="9" x2="12" y2="13"/>
                <line x1="12" y1="17" x2="12.01" y2="17"/>
              </svg>
            </span>
            <div class="alert-text">
              <strong>Low TPS Alert: Server ticking slowly</strong>
              <small>TPS has dropped below 15.0 for &gt;10s.</small>
            </div>
          </div>
        {/if}

        {#if mobileNavTab === 'server'}
          <div class="mobile-panel" id="mobile-panel-server">
            <!-- Primary Action Card -->
            <div class="mobile-card mobile-action-card">
              <div class="mobile-action-header">
                <h4>Server Lifecycle</h4>
                <button
                  type="button"
                  class="secondary-button small mobile-refresh-btn"
                  disabled={serverBusy}
                  on:click={() => void refreshServerStatus()}
                  aria-label="Refresh status"
                >
                  ↻ Refresh
                </button>
              </div>

              <div class="mobile-action-button-wrap">
                {#if serverState === 'running'}
                  <button
                    type="button"
                    class="danger-button mobile-primary-btn"
                    disabled={serverBusy}
                    on:click={toggleServer}
                  >
                    {serverBusy ? 'Stopping Server...' : '■ Stop Server'}
                  </button>
                {:else if serverState === 'crashed'}
                  <button
                    type="button"
                    class="save-button mobile-primary-btn restart-accent"
                    disabled={serverBusy}
                    on:click={toggleServer}
                  >
                    {serverBusy ? 'Restarting Server...' : '⚠ Restart Server'}
                  </button>
                {:else}
                  <button
                    type="button"
                    class="save-button mobile-primary-btn"
                    disabled={serverBusy}
                    on:click={toggleServer}
                  >
                    {serverBusy ? 'Starting Server...' : '▶ Start Server'}
                  </button>
                {/if}
              </div>
            </div>

            <!-- Quick Telemetry Grid -->
            <div class="mobile-card">
              <h4>Real-time Telemetry</h4>
              <div class="mobile-telemetry-grid">
                <div class="mobile-telemetry-item">
                  <span class="stat-kicker">Tick Rate</span>
                  <div class="stat-value">{resourceMetrics?.tps?.toFixed(1) ?? '20.0'}</div>
                  <span class="stat-sub">TPS (target 20.0)</span>
                </div>
                <div class="mobile-telemetry-item">
                  <span class="stat-kicker">CPU Usage</span>
                  <div class="stat-value">{resourceMetrics?.cpu_percent?.toFixed(1) ?? '0.0'}%</div>
                  <span class="stat-sub">Processor Load</span>
                </div>
                <div class="mobile-telemetry-item">
                  <span class="stat-kicker">Process RAM</span>
                  <div class="stat-value">{resourceMetrics?.memory_mb ?? 512} <small>MiB</small></div>
                  <span class="stat-sub">Resident Set (RSS)</span>
                </div>
                <div class="mobile-telemetry-item">
                  <span class="stat-kicker">JVM Heap</span>
                  <div class="stat-value">{resourceMetrics?.heap_used_mb ?? 256} <small>MiB</small></div>
                  <span class="stat-sub">Allocated: {resourceMetrics?.heap_max_mb ?? jvmSettings.ramMb} MiB</span>
                </div>
              </div>
              <div class="mobile-process-info">
                <span>PID: #{resourceMetrics?.pid ?? 0}</span>
                <span>Uptime: {formatUptime(resourceMetrics?.uptime_seconds ?? 0)}</span>
              </div>
            </div>

            <!-- Player Join Address -->
            {#if joinInfo}
              <div class="mobile-card">
                <h4>Player Join Address (Plain TCP)</h4>
                <div class="mobile-join-row">
                  <code class="mobile-join-code">{joinInfo.qr_payload}</code>
                  <button
                    type="button"
                    class="secondary-button small"
                    on:click={copyJoinAddress}
                  >
                    {copiedAddress ? 'Copied!' : 'Copy'}
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {:else if mobileNavTab === 'console'}
          <div class="mobile-panel" id="mobile-panel-console">
            <div class="mobile-card mobile-console-card">
              <div class="mobile-console-header">
                <h4>Live Console Stream</h4>
                <button
                  type="button"
                  class="secondary-button small"
                  on:click={clearConsole}
                >Clear</button>
              </div>

              <div class="mobile-terminal" role="log" aria-live="polite">
                {#if consoleEvents.length === 0}
                  <div class="console-empty">No console output recorded.</div>
                {:else}
                  {#each consoleEvents.slice(-80) as event (event.timestamp + event.message)}
                    <div class="console-line line-{event.level.toLowerCase()}">
                      <span class="console-time">{new Date(event.timestamp).toLocaleTimeString()}</span>
                      <span class="console-level-badge level-{event.level.toLowerCase()}">{event.level}</span>
                      <span class="console-text">{event.message}</span>
                    </div>
                  {/each}
                {/if}
              </div>

              <form class="mobile-cmd-form" on:submit|preventDefault={submitServerCommand}>
                <input
                  type="text"
                  placeholder="e.g. say hello, time set day"
                  bind:value={serverCommand}
                  class="mobile-cmd-input"
                  aria-label="Minecraft command"
                />
                <button
                  type="submit"
                  class="save-button mobile-cmd-submit"
                  disabled={!serverCommand.trim()}
                >Send</button>
              </form>

              <div class="mobile-quick-cmds">
                <button type="button" class="quick-cmd-chip" on:click={() => { serverCommand = 'list'; void submitServerCommand(); }}>/list</button>
                <button type="button" class="quick-cmd-chip" on:click={() => { serverCommand = 'time set day'; void submitServerCommand(); }}>/time set day</button>
                <button type="button" class="quick-cmd-chip" on:click={() => { serverCommand = 'weather clear'; void submitServerCommand(); }}>/weather clear</button>
                <button type="button" class="quick-cmd-chip" on:click={() => { serverCommand = 'save-all'; void submitServerCommand(); }}>/save-all</button>
              </div>
            </div>
          </div>
        {:else if mobileNavTab === 'players'}
          <div class="mobile-panel" id="mobile-panel-players">
            <div class="mobile-card">
              <h4>Whitelisted Players ({playerLists.whitelist.length})</h4>
              <form class="mobile-player-add-form" on:submit|preventDefault={() => void addPlayerToWhitelist()}>
                <input
                  type="text"
                  placeholder="Player username"
                  bind:value={newWhitelistName}
                  aria-label="Player name to whitelist"
                />
                <button
                  type="submit"
                  class="save-button"
                  disabled={!newWhitelistName.trim() || playersLoading}
                >Add</button>
              </form>

              {#if playerLists.whitelist.length === 0}
                <p class="empty-state">No players on whitelist.</p>
              {:else}
                <ul class="mobile-player-list" role="list">
                  {#each playerLists.whitelist as player}
                    <li class="mobile-player-item">
                      <div class="player-info">
                        <strong>{player.name}</strong>
                        <small>{player.uuid}</small>
                      </div>
                      <button
                        type="button"
                        class="secondary-button small"
                        aria-label={`Remove ${player.name}`}
                        on:click={() => void removePlayerFromWhitelist(player)}
                      >Remove</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <!-- Fixed Persistent Bottom Nav (Touch targets >= 44px, <= 2 levels deep) -->
      <div class="mobile-bottom-nav" aria-label="Mobile companion navigation" role="tablist">
        <button
          id="mobile-tab-server"
          type="button"
          role="tab"
          class="mobile-nav-item"
          class:active={mobileNavTab === 'server'}
          aria-selected={mobileNavTab === 'server'}
          on:click={() => (mobileNavTab = 'server')}
        >
          <span class="nav-icon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect width="20" height="14" x="2" y="3" rx="2"/>
              <line x1="8" x2="16" y1="21" y2="21"/>
              <line x1="12" x2="12" y1="17" y2="21"/>
            </svg>
          </span>
          <span class="nav-label">Server</span>
        </button>
        <button
          id="mobile-tab-console"
          type="button"
          role="tab"
          class="mobile-nav-item"
          class:active={mobileNavTab === 'console'}
          aria-selected={mobileNavTab === 'console'}
          on:click={() => (mobileNavTab = 'console')}
        >
          <span class="nav-icon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="4 17 10 11 4 5"/>
              <line x1="12" x2="20" y1="19" y2="19"/>
            </svg>
          </span>
          <span class="nav-label">Console</span>
        </button>
        <button
          id="mobile-tab-players"
          type="button"
          role="tab"
          class="mobile-nav-item"
          class:active={mobileNavTab === 'players'}
          aria-selected={mobileNavTab === 'players'}
          on:click={() => (mobileNavTab = 'players')}
        >
          <span class="nav-icon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
              <circle cx="9" cy="7" r="4"/>
              <path d="M22 21v-2a4 4 0 0 0-3-3.87"/>
              <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
            </svg>
          </span>
          <span class="nav-label">Players ({playerLists.whitelist.length})</span>
        </button>
      </div>
    {/if}
  </main>
{:else}
  <main class="app-shell">
  <header class="topbar">
    <div class="topbar-left">
      <a class="brand" href="/" aria-label="Redstone home">
        <span class="brand-mark" aria-hidden="true">R</span>
        <span>REDSTONE</span>
      </a>
      <span class="topbar-server-badge">
        <span
          class="server-status-dot"
          class:dot-running={serverState === 'running'}
          class:dot-crashed={serverState === 'crashed'}
          class:dot-stopped={serverState === 'stopped'}
          title={serverState}
        ></span>
        <strong>{settings?.instance_name ?? 'No server selected'}</strong>
      </span>
    </div>
    <div class="topbar-right">
      <button
        type="button"
        class="mode-switch-btn"
        on:click={() => (appMode = 'mobile')}
        title="Switch to Mobile Companion View"
        aria-label="Switch to Mobile Companion View"
      >
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <rect width="14" height="20" x="5" y="2" rx="2" ry="2"/>
          <line x1="12" x2="12.01" y1="18" y2="18"/>
        </svg>
        <span>Mobile View</span>
      </button>
      <span class="device-status"><span aria-hidden="true"></span> ON THIS DEVICE</span>
    </div>
  </header>

  <div class="app-body">
    <!-- Left Sidebar: instances/servers panel sticking to the left side, 100% remaining Y-axis space, not collapsible -->
    <aside class="sidebar-servers" aria-label="Servers sidebar">
      <div class="servers-panel-card">
        <div class="servers-panel-header">
          <div class="header-left">
            <svg class="rack-icon" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true">
              <rect x="2" y="3" width="20" height="7" rx="1.5" fill="#242831" stroke="#4b5563"/>
              <rect x="2" y="13" width="20" height="7" rx="1.5" fill="#242831" stroke="#4b5563"/>
              <circle cx="5.5" cy="6.5" r="1.2" fill="#22c55e"/>
              <circle cx="8.5" cy="6.5" r="1.2" fill="#ef4444"/>
              <circle cx="5.5" cy="16.5" r="1.2" fill="#22c55e"/>
              <circle cx="8.5" cy="16.5" r="1.2" fill="#22c55e"/>
              <line x1="13" y1="6.5" x2="19" y2="6.5" stroke="#6b7280" stroke-linecap="round"/>
              <line x1="13" y1="16.5" x2="19" y2="16.5" stroke="#6b7280" stroke-linecap="round"/>
            </svg>
            <h2 class="servers-title">Servers</h2>
          </div>
          <div class="header-right options-menu-container">
            <button
              type="button"
              class="server-tool-btn add-server-btn"
              on:click={() => (showCreateInstanceForm = !showCreateInstanceForm)}
              title="Add Server"
              aria-label="Add Server"
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <line x1="12" y1="5" x2="12" y2="19"/>
                <line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
            </button>
            <button
              type="button"
              class="server-tool-btn hamburger-btn"
              title="Options"
              aria-label="Options"
              aria-haspopup="true"
              aria-expanded={showOptionsDropdown}
              on:click|stopPropagation={() => (showOptionsDropdown = !showOptionsDropdown)}
            >
              <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <line x1="3" x2="21" y1="6" y2="6"/>
                <line x1="3" x2="21" y1="12" y2="12"/>
                <line x1="3" x2="21" y1="18" y2="18"/>
              </svg>
            </button>
            {#if showOptionsDropdown}
              <div class="options-dropdown-menu" role="menu" aria-label="Application options">
                <!-- Theme Switch: Light / Dark with SVG logos -->
                <div class="theme-switch-row" role="group" aria-label="Theme mode switch">
                  <button
                    type="button"
                    class="theme-switch-btn theme-light"
                    class:active={themeMode === 'light'}
                    title="Switch to light mode"
                    aria-label="Light mode"
                    on:click|stopPropagation={() => setTheme('light')}
                  >
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <circle cx="12" cy="12" r="5"/>
                      <line x1="12" y1="1" x2="12" y2="3"/>
                      <line x1="12" y1="21" x2="12" y2="23"/>
                      <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
                      <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
                      <line x1="1" y1="12" x2="3" y2="12"/>
                      <line x1="21" y1="12" x2="23" y2="12"/>
                      <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
                      <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
                    </svg>
                    <span>Light</span>
                  </button>
                  <button
                    type="button"
                    class="theme-switch-btn theme-dark"
                    class:active={themeMode === 'dark' || themeMode === 'system'}
                    title="Switch to dark mode"
                    aria-label="Dark mode"
                    on:click|stopPropagation={() => setTheme('dark')}
                  >
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
                    </svg>
                    <span>Dark</span>
                  </button>
                </div>
                <div class="dropdown-divider"></div>
                <button
                  type="button"
                  class="dropdown-menu-item"
                  role="menuitem"
                  on:click|stopPropagation={() => {
                    showOptionsDropdown = false;
                    showPreferencesModal = true;
                  }}
                >
                  <span class="item-label">Preferences</span>
                  <kbd class="item-shortcut">Ctrl+,</kbd>
                </button>
                <button
                  type="button"
                  class="dropdown-menu-item"
                  role="menuitem"
                  on:click|stopPropagation={() => {
                    showOptionsDropdown = false;
                    showShortcutsModal = true;
                  }}
                >
                  <span class="item-label">Keyboard Shortcuts</span>
                  <kbd class="item-shortcut">Ctrl+?</kbd>
                </button>
                <button
                  type="button"
                  class="dropdown-menu-item"
                  role="menuitem"
                  on:click|stopPropagation={() => {
                    showOptionsDropdown = false;
                    showAboutModal = true;
                  }}
                >
                  <span class="item-label">About Redstone</span>
                </button>
              </div>
            {/if}
          </div>
        </div>

        {#if showCreateInstanceForm}
          <fieldset class="server-inline-create">
            <legend>New server</legend>
            <div class="server-create-inputs">
              <label class="form-field-dark">
                <span>Server name</span>
                <input bind:value={instanceName} type="text" minlength="1" maxlength="48" required placeholder="My Server" />
              </label>
              <label class="form-field-dark">
                <span>Version</span>
                <select bind:value={minecraftVersion}>
                  <option value="1.21.4">1.21.4</option>
                  <option value="1.21.1">1.21.1</option>
                  <option value="1.20.1">1.20.1</option>
                </select>
              </label>
              <label class="form-field-dark">
                <span>Loader</span>
                <select bind:value={loader}>
                  <option value="vanilla">Vanilla</option>
                  <option value="fabric">Fabric</option>
                  <option value="forge">Forge</option>
                  <option value="neoforge">NeoForge</option>
                  <option value="paper">Paper</option>
                  <option value="spigot">Spigot</option>
                  <option value="purpur">Purpur</option>
                  <option value="folia">Folia</option>
                  <option value="sponge">Sponge</option>
                </select>
              </label>
            </div>
            <div class="server-create-actions">
              <button class="save-button small" type="button" disabled={saving} on:click={() => {
                void createNewInstance();
                showCreateInstanceForm = false;
              }}>
                {saving ? 'Creating...' : 'Create Server'}
              </button>
              <button class="secondary-button small" type="button" on:click={() => (showCreateInstanceForm = false)}>
                Cancel
              </button>
            </div>
          </fieldset>
        {/if}

        <div class="servers-card-list" role="list">
          {#each instances as instance (instance.id)}
            <div
              class="server-item-card"
              class:active={settings?.instance_name === instance.name}
              role="button"
              tabindex="0"
              on:click={() => selectInstance(instance)}
              on:keydown={(e) => { if (e.key === 'Enter' || e.key === ' ') selectInstance(instance); }}
              aria-label={`Select ${instance.name}`}
            >
              <div class="server-item-avatar" style="background: {avatarColorFor(instance.name)}">
                <span>{avatarInitialsFor(instance.name)}</span>
              </div>
              <div class="server-item-info">
                <strong class="server-item-name">{instance.name}</strong>
                <span class="server-item-meta">{instance.minecraft_version} · {formatLoader(instance.loader)}</span>
              </div>
              <div class="server-item-status">
                <span
                  class="server-status-dot"
                  class:dot-running={settings?.instance_name === instance.name && serverState === 'running'}
                  class:dot-crashed={settings?.instance_name === instance.name && serverState === 'crashed'}
                  class:dot-stopped={settings?.instance_name !== instance.name || serverState === 'stopped'}
                  title={settings?.instance_name === instance.name ? serverState : 'stopped'}
                ></span>
              </div>
            </div>
          {:else}
            <div class="servers-empty">
              <p>No servers found.</p>
              <button
                type="button"
                class="save-button small"
                on:click={() => (showCreateInstanceForm = true)}
              >
                + Add your first server
              </button>
            </div>
          {/each}
        </div>

        {#if settings?.instance_name}
          <div class="server-sidebar-actions" role="group" aria-label="Selected server actions">
            <button
              type="button"
              class="sidebar-action-btn duplicate-instance-btn"
              disabled={instanceActionBusy}
              on:click={() => void duplicateSelectedInstance()}
              title="Duplicate selected server without copying world data"
            >
              Duplicate
            </button>
            <button
              type="button"
              class="sidebar-action-btn danger delete-instance-btn"
              disabled={instanceActionBusy || serverState === 'running'}
              on:click={() => void deleteSelectedInstance()}
              title={serverState === 'running' ? 'Stop server before deleting' : 'Delete selected server'}
            >
              Delete
            </button>
          </div>
        {/if}
      </div>
    </aside>

    <div class="main-workspace">
      <section class="setup-view" aria-labelledby="page-title">
        <div class="step-label"><span>01</span> LOCAL SERVER WORKSPACE</div>
        <div class="intro">
          <h1 id="page-title">Redstone control room</h1>
          <p>Manage your instances, mods, and server settings on this device.</p>
        </div>

        <div class="workspace-tabs" role="tablist" aria-label="Redstone sections">
          <button
            id="tab-catalog"
            class:active={activeTab === 'catalog'}
            role="tab"
            aria-selected={activeTab === 'catalog'}
            aria-controls="panel-catalog"
            tabindex={activeTab === 'catalog' ? 0 : -1}
            type="button"
            on:click={() => (activeTab = 'catalog')}
            on:keydown={(event) => handleTabKeydown(event, 'catalog')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/>
              <path d="m3.3 7 8.7 5 8.7-5"/>
              <path d="M12 22V12"/>
            </svg>
            Mods
          </button>
          <button
            id="tab-installed"
            class:active={activeTab === 'installed'}
            role="tab"
            aria-selected={activeTab === 'installed'}
            aria-controls="panel-installed"
            tabindex={activeTab === 'installed' ? 0 : -1}
            type="button"
            on:click={() => (activeTab = 'installed')}
            on:keydown={(event) => handleTabKeydown(event, 'installed')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M12 2v6"/>
              <path d="m19 13-4-4"/>
              <path d="m5 13 4-4"/>
              <path d="M9 13v6a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2v-6"/>
              <path d="M12 17v4"/>
            </svg>
            Installed mods{#if modUpdates.length}<span class="update-count">{modUpdates.length}</span>{/if}
          </button>
          <button
            id="tab-server"
            class:active={activeTab === 'server'}
            role="tab"
            aria-selected={activeTab === 'server'}
            aria-controls="panel-server"
            tabindex={activeTab === 'server' ? 0 : -1}
            type="button"
            on:click={() => {
              activeTab = 'server';
              void refreshJoinInfo();
            }}
            on:keydown={(event) => handleTabKeydown(event, 'server')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
              <circle cx="12" cy="12" r="3"/>
            </svg>
            Server management
          </button>
          <button
            id="tab-monitor"
            class:active={activeTab === 'monitor'}
            role="tab"
            aria-selected={activeTab === 'monitor'}
            aria-controls="panel-monitor"
            tabindex={activeTab === 'monitor' ? 0 : -1}
            type="button"
            on:click={() => {
              activeTab = 'monitor';
              void refreshServerStatus();
              void refreshResourceMetrics();
            }}
            on:keydown={(event) => handleTabKeydown(event, 'monitor')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
            </svg>
            Monitor
          </button>
          <button
            id="tab-files"
            class:active={activeTab === 'files'}
            role="tab"
            aria-selected={activeTab === 'files'}
            aria-controls="panel-files"
            tabindex={activeTab === 'files' ? 0 : -1}
            type="button"
            on:click={() => {
              activeTab = 'files';
              void loadFiles();
            }}
            on:keydown={(event) => handleTabKeydown(event, 'files')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z"/>
            </svg>
            Files
          </button>
          <button
            id="tab-players"
            class:active={activeTab === 'players'}
            role="tab"
            aria-selected={activeTab === 'players'}
            aria-controls="panel-players"
            tabindex={activeTab === 'players' ? 0 : -1}
            type="button"
            on:click={() => {
              activeTab = 'players';
              void loadPlayers();
            }}
            on:keydown={(event) => handleTabKeydown(event, 'players')}
          >
            <svg class="tab-svg-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/>
              <circle cx="9" cy="7" r="4"/>
              <path d="M22 21v-2a4 4 0 0 0-3-3.87"/>
              <path d="M16 3.13a4 4 0 0 1 0 7.75"/>
            </svg>
            Players
          </button>
        </div>

    {#if loading}
      <p class="feedback" role="status">Loading local settings...</p>
    {:else if settings}
      <div class="settings-form">
        {#if activeTab === 'server'}
        <div id="panel-server" class="tab-panel" role="tabpanel" aria-labelledby="tab-server">
        <form class="server-settings-form" on:submit|preventDefault={submitSettings}>
        <fieldset>
          <legend>Default network access</legend>
          <p class="field-hint">Local access keeps new servers on your network until you choose otherwise.</p>
          <div class="access-options" role="group" aria-label="Default network access">
            <button
              class:chosen={settings.lan_only}
              type="button"
              aria-pressed={settings.lan_only}
              on:click={() => setLanOnly(true)}
            >
              <span class="option-indicator" aria-hidden="true"></span>
              <span class="option-copy"><strong>LAN only</strong><small>Local network</small></span>
            </button>
            <button
              class:chosen={!settings.lan_only}
              type="button"
              aria-pressed={!settings.lan_only}
              on:click={() => setLanOnly(false)}
            >
              <span class="option-indicator" aria-hidden="true"></span>
              <span class="option-copy"><strong>Online</strong><small>Public access</small></span>
            </button>
          </div>
        </fieldset>

        <section class="join-card" aria-labelledby="join-card-title">
          <div class="join-card-header">
            <div>
              <span class="status-kicker">CONNECTION & SHARING</span>
              <h3 id="join-card-title">Server Join Address</h3>
            </div>
            <span class="status-pill" class:running={!settings.lan_only}>
              {settings.lan_only ? 'LAN ONLY' : 'PUBLIC'}
            </span>
          </div>

          <div class="join-address-box">
            <div class="join-address-text">
              {#if joinInfo}
                <strong class="join-host">{joinInfo.address}</strong>
                {#if joinInfo.port !== 25565 || joinInfo.is_lan}
                  <span class="join-port">:{joinInfo.port}</span>
                {/if}
              {:else}
                <span class="join-loading">Loading join address...</span>
              {/if}
            </div>
            <div class="join-actions">
              <button
                type="button"
                class="secondary-button small"
                disabled={!joinInfo}
                on:click={copyJoinAddress}
              >
                {copiedAddress ? 'Copied!' : 'Copy Address'}
              </button>
              <button
                type="button"
                class="secondary-button small"
                disabled={!joinInfo}
                on:click={() => (showQrModal = !showQrModal)}
              >
                {showQrModal ? 'Hide QR' : 'Show QR'}
              </button>
            </div>
          </div>

          {#if showQrModal && joinInfo}
            <div class="qr-modal-panel">
              <p class="field-hint">Scan with mobile devices or share with LAN players:</p>
              <div class="qr-display">
                {@html generateQrSvg(joinInfo.qr_payload)}
              </div>
              <p class="qr-payload-text"><code>{joinInfo.qr_payload}</code></p>
            </div>
          {/if}
        </section>

        {#if !settings.lan_only}
          <fieldset class="dns-fields">
            <legend>Dynamic DNS Configuration</legend>
            <p class="field-hint">
              Keep your server accessible using DuckDNS or Cloudflare DNS. Secrets are stored securely in your OS keychain.
            </p>
            <label class="form-field">
              <span>DNS Provider</span>
              <select
                value={settings.dns_provider ?? 'manual'}
                on:change={(event) => onDnsProviderChange(event.currentTarget.value)}
              >
                <option value="manual">Manual DNS (hostname only)</option>
                <option value="duckdns">DuckDNS</option>
                <option value="desec">deSEC (desec.io)</option>
                <option value="cloudflare">Cloudflare</option>
                <option value="playit">Playit.gg (tunnel)</option>
              </select>
            </label>

            {#if (settings.dns_provider ?? 'manual') === 'duckdns'}
              <div class="address-fields">
                <label class="form-field">
                  <span>DuckDNS Subdomain</span>
                  <input
                    value={settings.dns_subdomain ?? 'play'}
                    on:input={(event) => updateDnsSettings('dns_subdomain', event.currentTarget.value)}
                    type="text"
                    maxlength="63"
                    autocomplete="off"
                    placeholder="mycoolserver"
                  />
                </label>
                <span class="hostname-dot" aria-hidden="true">.</span>
                <label class="form-field">
                  <span>Domain</span>
                  <input
                    value="duckdns.org"
                    disabled
                    type="text"
                  />
                </label>
              </div>

              <div class="secret-field-group">
                <label class="form-field">
                  <span>DuckDNS Token (OS Keychain)</span>
                  <div class="password-input-row">
                    <input
                      type={dnsSecretMasked ? 'password' : 'text'}
                      bind:value={dnsSecretToken}
                      placeholder="e.g. 8a9b7c6d-..."
                      autocomplete="off"
                    />
                    <button
                      type="button"
                      class="secondary-button small"
                      on:click={() => (dnsSecretMasked = !dnsSecretMasked)}
                    >
                      {dnsSecretMasked ? 'Show' : 'Hide'}
                    </button>
                    <button
                      type="button"
                      class="save-button small"
                      disabled={!dnsSecretToken.trim() || dnsSecretBusy}
                      on:click={saveDnsSecret}
                    >
                      Save to Keychain
                    </button>
                  </div>
                </label>
              </div>

              <div class="dns-sync-actions">
                <button
                  type="button"
                  class="secondary-button"
                  disabled={dnsSyncBusy}
                  on:click={syncDns}
                >
                  {dnsSyncBusy ? 'Syncing...' : 'Sync DuckDNS Now'}
                </button>
              </div>
            {:else if (settings.dns_provider ?? 'manual') === 'desec'}
              <div class="address-fields">
                <label class="form-field">
                  <span>deSEC Subdomain</span>
                  <input
                    value={settings.dns_subdomain ?? 'play'}
                    on:input={(event) => updateDnsSettings('dns_subdomain', event.currentTarget.value)}
                    type="text"
                    maxlength="63"
                    autocomplete="off"
                    placeholder="play"
                  />
                </label>
                <span class="hostname-dot" aria-hidden="true">.</span>
                <label class="form-field">
                  <span>Domain</span>
                  <input
                    value={settings.dns_domain ?? 'dedyn.io'}
                    on:input={(event) => updateDnsSettings('dns_domain', event.currentTarget.value)}
                    type="text"
                    maxlength="253"
                    autocomplete="off"
                    placeholder="dedyn.io"
                  />
                </label>
              </div>

              <div class="secret-field-group">
                <label class="form-field">
                  <span>deSEC Token (OS Keychain)</span>
                  <div class="password-input-row">
                    <input
                      type={dnsSecretMasked ? 'password' : 'text'}
                      bind:value={dnsSecretToken}
                      placeholder="deSEC API Token"
                      autocomplete="off"
                    />
                    <button
                      type="button"
                      class="secondary-button small"
                      on:click={() => (dnsSecretMasked = !dnsSecretMasked)}
                    >
                      {dnsSecretMasked ? 'Show' : 'Hide'}
                    </button>
                    <button
                      type="button"
                      class="save-button small"
                      disabled={!dnsSecretToken.trim() || dnsSecretBusy}
                      on:click={saveDnsSecret}
                    >
                      Save to Keychain
                    </button>
                  </div>
                </label>
              </div>

              <div class="dns-sync-actions">
                <button
                  type="button"
                  class="secondary-button"
                  disabled={dnsSyncBusy}
                  on:click={syncDns}
                >
                  {dnsSyncBusy ? 'Syncing...' : 'Sync deSEC DNS Now'}
                </button>
              </div>
            {:else if (settings.dns_provider ?? 'manual') === 'playit'}
              <div class="address-fields">
                <label class="form-field">
                  <span>Playit Tunnel Name / Subdomain</span>
                  <input
                    value={settings.dns_subdomain ?? 'play'}
                    on:input={(event) => updateDnsSettings('dns_subdomain', event.currentTarget.value)}
                    type="text"
                    maxlength="63"
                    autocomplete="off"
                    placeholder="play"
                  />
                </label>
                <span class="hostname-dot" aria-hidden="true">.</span>
                <label class="form-field">
                  <span>Tunnel Domain</span>
                  <input
                    value="gl.joinmc.link"
                    disabled
                    type="text"
                  />
                </label>
              </div>

              <div class="secret-field-group">
                <label class="form-field">
                  <span>Playit Secret Key (OS Keychain)</span>
                  <div class="password-input-row">
                    <input
                      type={dnsSecretMasked ? 'password' : 'text'}
                      bind:value={dnsSecretToken}
                      placeholder="Playit Secret Key"
                      autocomplete="off"
                    />
                    <button
                      type="button"
                      class="secondary-button small"
                      on:click={() => (dnsSecretMasked = !dnsSecretMasked)}
                    >
                      {dnsSecretMasked ? 'Show' : 'Hide'}
                    </button>
                    <button
                      type="button"
                      class="save-button small"
                      disabled={!dnsSecretToken.trim() || dnsSecretBusy}
                      on:click={saveDnsSecret}
                    >
                      Save to Keychain
                    </button>
                  </div>
                </label>
              </div>

              <div class="dns-sync-actions">
                <button
                  type="button"
                  class="secondary-button"
                  disabled={dnsSyncBusy}
                  on:click={syncDns}
                >
                  {dnsSyncBusy ? 'Syncing...' : 'Sync Playit Tunnel Now'}
                </button>
              </div>
            {:else if (settings.dns_provider ?? 'manual') === 'cloudflare'}
              <div class="address-fields">
                <label class="form-field">
                  <span>Subdomain</span>
                  <input
                    value={settings.dns_subdomain ?? 'play'}
                    on:input={(event) => updateDnsSettings('dns_subdomain', event.currentTarget.value)}
                    type="text"
                    maxlength="63"
                    autocomplete="off"
                    placeholder="play"
                  />
                </label>
                <span class="hostname-dot" aria-hidden="true">.</span>
                <label class="form-field">
                  <span>Domain</span>
                  <input
                    value={settings.dns_domain ?? ''}
                    on:input={(event) => updateDnsSettings('dns_domain', event.currentTarget.value)}
                    type="text"
                    maxlength="253"
                    autocomplete="off"
                    placeholder="example.com"
                  />
                </label>
              </div>

              <div class="secret-field-group">
                <label class="form-field">
                  <span>Cloudflare Zone ID</span>
                  <input
                    type="text"
                    bind:value={dnsZoneId}
                    placeholder="32-character hexadecimal Zone ID"
                    autocomplete="off"
                  />
                </label>

                <label class="form-field">
                  <span>Cloudflare API Token (OS Keychain)</span>
                  <div class="password-input-row">
                    <input
                      type={dnsSecretMasked ? 'password' : 'text'}
                      bind:value={dnsSecretToken}
                      placeholder="API Token with Zone.DNS edit permissions"
                      autocomplete="off"
                    />
                    <button
                      type="button"
                      class="secondary-button small"
                      on:click={() => (dnsSecretMasked = !dnsSecretMasked)}
                    >
                      {dnsSecretMasked ? 'Show' : 'Hide'}
                    </button>
                    <button
                      type="button"
                      class="save-button small"
                      disabled={!dnsSecretToken.trim() || dnsSecretBusy}
                      on:click={saveDnsSecret}
                    >
                      Save to Keychain
                    </button>
                  </div>
                </label>
              </div>

              <div class="dns-sync-actions">
                <button
                  type="button"
                  class="secondary-button"
                  disabled={dnsSyncBusy}
                  on:click={syncDns}
                >
                  {dnsSyncBusy ? 'Syncing...' : 'Sync Cloudflare DNS Now'}
                </button>
              </div>
            {:else}
              <div class="address-fields">
                <label class="form-field">
                  <span>Subdomain</span>
                  <input
                    value={settings.dns_subdomain ?? 'play'}
                    on:input={(event) => updateDnsSettings('dns_subdomain', event.currentTarget.value)}
                    type="text"
                    maxlength="63"
                    autocomplete="off"
                    placeholder="play"
                  />
                </label>
                <span class="hostname-dot" aria-hidden="true">.</span>
                <label class="form-field">
                  <span>Domain</span>
                  <input
                    value={settings.dns_domain ?? ''}
                    on:input={(event) => updateDnsSettings('dns_domain', event.currentTarget.value)}
                    type="text"
                    maxlength="253"
                    autocomplete="off"
                    placeholder="example.com"
                  />
                </label>
              </div>
            {/if}

            {#if dnsSyncMessage}
              <p class="catalog-message" role="status">{dnsSyncMessage}</p>
            {/if}
            {#if dnsSyncError}
              <p class="catalog-message error" role="alert">{dnsSyncError}</p>
            {/if}
          </fieldset>
        {/if}

        <section class="remote-card" aria-labelledby="remote-control-title">
          <div class="remote-header">
            <div class="remote-title-group">
              <span class="status-kicker">END-TO-END ENCRYPTED</span>
              <h3 id="remote-control-title">Mobile Remote Control (Iroh)</h3>
              <p class="field-hint">
                Control this server from your phone or tablet over a secure Iroh channel (Ed25519 verified).
                Vanilla Minecraft client connections stay on plain TCP and never touch Iroh.
              </p>
            </div>
            <div class="remote-top-actions">
              <button
                type="button"
                class="secondary-button small"
                on:click={() => (appMode = 'mobile')}
              >
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <rect width="14" height="20" x="5" y="2" rx="2" ry="2"/>
                  <line x1="12" x2="12.01" y1="18" y2="18"/>
                </svg>
                <span>Open Mobile View</span>
              </button>
              <button
                type="button"
                class="save-button small"
                disabled={remoteLoading}
                on:click={generateTicket}
              >
                {pairingTicket ? 'Regenerate Ticket' : 'Pair Mobile Device'}
              </button>
            </div>
          </div>

          {#if remoteMessage}
            <p class="catalog-message" role="status">{remoteMessage}</p>
          {/if}
          {#if remoteError}
            <p class="catalog-message error" role="alert">{remoteError}</p>
          {/if}

          {#if pairingTicket}
            <div class="pairing-ticket-box">
              <div class="ticket-qr-section">
                <div class="qr-display">
                  {@html generateQrSvg(pairingTicket.qr_payload)}
                </div>
                <div class="ticket-info">
                  <span class="status-kicker">ONE-TIME PAIRING CODE</span>
                  <div class="ticket-code-display">
                    <strong>{pairingTicket.ticket_code}</strong>
                  </div>
                  <p class="field-hint">Host Endpoint: <code>{pairingTicket.endpoint_id}</code></p>
                  <p class="field-hint">Scan with your mobile companion or enter this 6-character code.</p>
                </div>
              </div>
            </div>
          {/if}

          <div class="paired-devices-section">
            <span class="status-kicker">AUTHORIZED CLIENTS ({pairedDevices.length})</span>
            {#if pairedDevices.length === 0}
              <p class="empty-state">No remote devices paired yet. Click "Pair Mobile Device" to authorize a companion.</p>
            {:else}
              <ul class="paired-device-list" role="list">
                {#each pairedDevices as device}
                  <li class="paired-device-row">
                    <div class="paired-device-info">
                      <span class="device-dot dot-online" aria-hidden="true"></span>
                      <strong>{device}</strong>
                      <span class="device-badge">Authorized</span>
                    </div>
                    <button
                      type="button"
                      class="secondary-button small revoke-device-btn"
                      disabled={remoteLoading}
                      on:click={() => void revokeDevice(device)}
                    >
                      Revoke
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </section>

        <fieldset class="world-properties">
          <legend>World and server properties</legend>
          <p class="field-hint">Configure gameplay, environment, and world options with the interactable controls below. Changes apply on the next server start.</p>

          <div class="world-properties-gui">
            <!-- 1. World Identity -->
            <div class="props-section">
              <h4>World Identity &amp; MOTD</h4>
              <div class="props-grid-3">
                <label class="form-field">
                  <span>Server MOTD</span>
                  <input
                    type="text"
                    value={getProp('motd', 'A Redstone Minecraft Server')}
                    on:input={(e) => setProp('motd', e.currentTarget.value)}
                    placeholder="A Redstone Minecraft Server"
                  />
                </label>
                <label class="form-field">
                  <span>World Name</span>
                  <input
                    type="text"
                    value={getProp('level-name', 'world')}
                    on:input={(e) => setProp('level-name', e.currentTarget.value)}
                    placeholder="world"
                  />
                </label>
                <label class="form-field">
                  <span>World Seed</span>
                  <input
                    type="text"
                    value={getProp('level-seed', '')}
                    on:input={(e) => setProp('level-seed', e.currentTarget.value)}
                    placeholder="Leave blank for random"
                  />
                </label>
              </div>
            </div>

            <!-- 2. Gamemode & Difficulty -->
            <div class="props-section">
              <h4>Game Rules &amp; Modes</h4>
              <div class="props-grid-4">
                <label class="form-field">
                  <span>Default Gamemode</span>
                  <select
                    value={getProp('gamemode', 'survival')}
                    on:change={(e) => setProp('gamemode', e.currentTarget.value)}
                  >
                    <option value="survival">Survival</option>
                    <option value="creative">Creative</option>
                    <option value="adventure">Adventure</option>
                    <option value="spectator">Spectator</option>
                  </select>
                </label>
                <label class="form-field">
                  <span>Difficulty</span>
                  <select
                    value={getProp('difficulty', 'easy')}
                    on:change={(e) => setProp('difficulty', e.currentTarget.value)}
                  >
                    <option value="peaceful">Peaceful</option>
                    <option value="easy">Easy</option>
                    <option value="normal">Normal</option>
                    <option value="hard">Hard</option>
                  </select>
                </label>
                <div class="prop-toggle-item">
                  <span class="toggle-label">Hardcore Mode</span>
                  <button
                    type="button"
                    class="prop-toggle-btn"
                    class:active={isPropTrue('hardcore')}
                    aria-pressed={isPropTrue('hardcore')}
                    on:click={() => toggleProp('hardcore')}
                  >
                    <span class="toggle-dot" aria-hidden="true"></span>
                    <span>{isPropTrue('hardcore') ? 'Enabled' : 'Disabled'}</span>
                  </button>
                </div>
                <div class="prop-toggle-item">
                  <span class="toggle-label">Force Gamemode</span>
                  <button
                    type="button"
                    class="prop-toggle-btn"
                    class:active={isPropTrue('force-gamemode')}
                    aria-pressed={isPropTrue('force-gamemode')}
                    on:click={() => toggleProp('force-gamemode')}
                  >
                    <span class="toggle-dot" aria-hidden="true"></span>
                    <span>{isPropTrue('force-gamemode') ? 'Enabled' : 'Disabled'}</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- 3. Feature & Security Toggles -->
            <div class="props-section">
              <h4>Environment &amp; Security Toggles</h4>
              <div class="props-toggles-grid">
                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('pvp', 'true')}
                  aria-pressed={isPropTrue('pvp', 'true')}
                  on:click={() => toggleProp('pvp', 'true')}
                >
                  <span class="toggle-card-title">PvP Combat</span>
                  <span class="toggle-card-status">{isPropTrue('pvp', 'true') ? 'Allowed' : 'Disabled'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('online-mode', 'true')}
                  aria-pressed={isPropTrue('online-mode', 'true')}
                  on:click={() => toggleProp('online-mode', 'true')}
                >
                  <span class="toggle-card-title">Online Auth</span>
                  <span class="toggle-card-status">{isPropTrue('online-mode', 'true') ? 'Mojang' : 'Offline'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('white-list')}
                  aria-pressed={isPropTrue('white-list')}
                  on:click={() => toggleProp('white-list')}
                >
                  <span class="toggle-card-title">Whitelist</span>
                  <span class="toggle-card-status">{isPropTrue('white-list') ? 'Enforced' : 'Open'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('allow-flight')}
                  aria-pressed={isPropTrue('allow-flight')}
                  on:click={() => toggleProp('allow-flight')}
                >
                  <span class="toggle-card-title">Survival Flight</span>
                  <span class="toggle-card-status">{isPropTrue('allow-flight') ? 'Allowed' : 'Disallowed'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('enable-command-block')}
                  aria-pressed={isPropTrue('enable-command-block')}
                  on:click={() => toggleProp('enable-command-block')}
                >
                  <span class="toggle-card-title">Command Blocks</span>
                  <span class="toggle-card-status">{isPropTrue('enable-command-block') ? 'Active' : 'Disabled'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('spawn-monsters', 'true')}
                  aria-pressed={isPropTrue('spawn-monsters', 'true')}
                  on:click={() => toggleProp('spawn-monsters', 'true')}
                >
                  <span class="toggle-card-title">Spawn Monsters</span>
                  <span class="toggle-card-status">{isPropTrue('spawn-monsters', 'true') ? 'Spawning' : 'Off'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('spawn-animals', 'true')}
                  aria-pressed={isPropTrue('spawn-animals', 'true')}
                  on:click={() => toggleProp('spawn-animals', 'true')}
                >
                  <span class="toggle-card-title">Spawn Animals</span>
                  <span class="toggle-card-status">{isPropTrue('spawn-animals', 'true') ? 'Spawning' : 'Off'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('spawn-npcs', 'true')}
                  aria-pressed={isPropTrue('spawn-npcs', 'true')}
                  on:click={() => toggleProp('spawn-npcs', 'true')}
                >
                  <span class="toggle-card-title">Spawn NPCs</span>
                  <span class="toggle-card-status">{isPropTrue('spawn-npcs', 'true') ? 'Spawning' : 'Off'}</span>
                </button>

                <button
                  type="button"
                  class="prop-toggle-card"
                  class:checked={isPropTrue('generate-structures', 'true')}
                  aria-pressed={isPropTrue('generate-structures', 'true')}
                  on:click={() => toggleProp('generate-structures', 'true')}
                >
                  <span class="toggle-card-title">Structures</span>
                  <span class="toggle-card-status">{isPropTrue('generate-structures', 'true') ? 'Generated' : 'Flat'}</span>
                </button>
              </div>
            </div>

            <!-- 4. Performance & Distance Sliders -->
            <div class="props-section">
              <h4>Capacity &amp; Distances</h4>
              <div class="props-sliders-grid">
                <label class="form-field slider-field">
                  <div class="slider-header">
                    <span>Max Players</span>
                    <strong>{getProp('max-players', '20')}</strong>
                  </div>
                  <input
                    type="range"
                    min="1"
                    max="100"
                    step="1"
                    value={parseInt(getProp('max-players', '20'), 10) || 20}
                    on:input={(e) => setProp('max-players', e.currentTarget.value)}
                  />
                </label>

                <label class="form-field slider-field">
                  <div class="slider-header">
                    <span>View Distance</span>
                    <strong>{getProp('view-distance', '10')} chunks</strong>
                  </div>
                  <input
                    type="range"
                    min="3"
                    max="32"
                    step="1"
                    value={parseInt(getProp('view-distance', '10'), 10) || 10}
                    on:input={(e) => setProp('view-distance', e.currentTarget.value)}
                  />
                </label>

                <label class="form-field slider-field">
                  <div class="slider-header">
                    <span>Simulation Distance</span>
                    <strong>{getProp('simulation-distance', '10')} chunks</strong>
                  </div>
                  <input
                    type="range"
                    min="3"
                    max="32"
                    step="1"
                    value={parseInt(getProp('simulation-distance', '10'), 10) || 10}
                    on:input={(e) => setProp('simulation-distance', e.currentTarget.value)}
                  />
                </label>

                <label class="form-field">
                  <span>Server Port</span>
                  <input
                    type="number"
                    min="1024"
                    max="65535"
                    value={getProp('server-port', '25565')}
                    on:input={(e) => setProp('server-port', e.currentTarget.value)}
                  />
                </label>
              </div>
            </div>

            <!-- 5. Advanced / Mod Properties -->
            <details class="advanced-props-details">
              <summary>Advanced &amp; Mod Properties ({customPropEntries.length})</summary>
              <div class="advanced-props-body">
                <p class="field-hint">Extra properties defined by mods or advanced server configurations.</p>
                {#if customPropEntries.length > 0}
                  <div class="custom-props-list">
                    {#each customPropEntries as [key, value] (key)}
                      <div class="custom-prop-row">
                        <span class="custom-prop-key" title={key}>{key}</span>
                        <input
                          type="text"
                          class="custom-prop-input"
                          {value}
                          on:input={(e) => setProp(key, e.currentTarget.value)}
                        />
                        <button
                          type="button"
                          class="secondary-button small remove-prop-btn"
                          on:click={() => removeCustomProp(key)}
                          title="Remove property"
                        >&times;</button>
                      </div>
                    {/each}
                  </div>
                {:else}
                  <p class="empty-state">No custom properties set.</p>
                {/if}

                <div class="add-prop-row">
                  <input
                    type="text"
                    placeholder="New property key (e.g. motd-2)"
                    bind:value={customPropertyKey}
                  />
                  <input
                    type="text"
                    placeholder="Value"
                    bind:value={customPropertyValue}
                  />
                  <button
                    type="button"
                    class="secondary-button small"
                    disabled={!customPropertyKey.trim()}
                    on:click={addCustomProp}
                  >+ Add Property</button>
                </div>
              </div>
            </details>
          </div>

          <div class="world-properties-actions">
            <button class="secondary-button" type="button" disabled={!settings.instance_name || worldSaveBusy} on:click={() => refreshWorldProperties()}>
              Reload properties
            </button>
            <button class="save-button" type="button" disabled={!settings.instance_name || worldSaveBusy} on:click={() => saveWorldProperties()}>
              {worldSaveBusy ? 'Saving...' : 'Save properties'}
              <span aria-hidden="true">&#8594;</span>
            </button>
          </div>
          {#if worldMessage}<p class="catalog-message" role="status">{worldMessage}</p>{/if}
        </fieldset>

        <fieldset class="jvm-settings">
          <legend>RAM and JVM arguments</legend>
          <p class="field-hint">RAM is applied as <code>-Xmx</code> on the next start. The warning threshold is 80% of detected system memory.</p>
          <label class="form-field">
            <span>Maximum RAM: {jvmSettings.ramMb} MiB</span>
            <input bind:value={jvmSettings.ramMb} type="range" min="512" max={memoryMb} step="256" />
          </label>
          {#if jvmSettings.ramMb > memoryMb * 0.8}
            <p class="catalog-message error" role="alert">This allocation exceeds 80% of detected system memory ({memoryMb} MiB).</p>
          {/if}
          <div class="jvm-presets" role="group" aria-label="JVM preset">
            <button class:chosen={jvmSettings.preset === 'Custom'} type="button" on:click={() => chooseJvmPreset('Custom')}>Custom</button>
            <button class:chosen={jvmSettings.preset === 'Performance'} type="button" on:click={() => chooseJvmPreset('Performance')}>Performance</button>
          </div>
          <label class="form-field">
            <span>Custom JVM arguments, one per line</span>
            <textarea bind:value={jvmSettings.custom} rows="5" spellcheck="false" placeholder="-Xms2G\n-Dfoo=bar"></textarea>
          </label>
          <div class="world-properties-actions">
            <button class="save-button" type="button" disabled={!settings.instance_name || jvmSaveBusy} on:click={() => saveJvmSettings()}>
              {jvmSaveBusy ? 'Saving...' : 'Save JVM settings'}
              <span aria-hidden="true">&#8594;</span>
            </button>
          </div>
          {#if jvmMessage}<p class="catalog-message" role="status">{jvmMessage}</p>{/if}
        </fieldset>

        <section class="provisioning-card" aria-labelledby="provisioning-title">
          <div class="provisioning-header">
            <div>
              <span class="status-kicker">RUNTIME PROVISIONING &amp; COMPLIANCE</span>
              <h3 id="provisioning-title">Mojang Server Runtime &amp; EULA</h3>
              <p class="field-hint">
                Redstone never bundles server binaries or auto-accepts legal terms. Download the official jar from Mojang and confirm the EULA before launching.
              </p>
            </div>
          </div>

          <div class="provisioning-grid">
            <div class="provision-step">
              <div class="provision-step-top">
                <strong>Java {setupStatus?.required_java_major ?? 21}+</strong>
                <span
                  class="status-pill"
                  class:running={setupStatus !== null && setupStatus.detected_java_major !== null && setupStatus.detected_java_major >= setupStatus.required_java_major}
                  class:crashed={setupStatus !== null && (setupStatus.detected_java_major === null || setupStatus.detected_java_major < setupStatus.required_java_major)}
                >
                  {setupStatus && setupStatus.detected_java_major !== null && setupStatus.detected_java_major >= setupStatus.required_java_major
                    ? 'COMPATIBLE'
                    : 'CHECK JAVA'}
                </span>
              </div>
              <p class="field-hint">
                {setupStatus?.detected_java_major
                  ? `Detected system Java ${setupStatus.detected_java_major}.`
                  : 'System Java runtime not detected in PATH.'}
              </p>
              {#if !setupStatus?.detected_java_major || setupStatus.detected_java_major < (setupStatus?.required_java_major ?? 21)}
                <a
                  class="provision-link"
                  href="https://adoptium.net/temurin/releases/"
                  target="_blank"
                  rel="noreferrer"
                >
                  Install Adoptium Temurin &rarr;
                </a>
              {/if}
            </div>

            <div class="provision-step">
              <div class="provision-step-top">
                <strong>Official server.jar</strong>
                <span class="status-pill" class:running={Boolean(setupStatus?.server_jar_present)}>
                  {setupStatus?.server_jar_present ? 'READY' : 'MISSING'}
                </span>
              </div>
              <p class="field-hint">
                {setupStatus?.server_jar_present
                  ? 'Official server.jar is present in the instance folder.'
                  : `Fetch Minecraft ${minecraftVersion} server.jar directly from Mojang.`}
              </p>
              <button
                type="button"
                class="secondary-button small download-jar-btn"
                disabled={!settings.instance_name || provisionBusy || Boolean(setupStatus?.server_jar_present)}
                on:click={() => void provisionServerJar()}
              >
                {setupStatus?.server_jar_present
                  ? 'server.jar Ready'
                  : provisionBusy
                    ? 'Downloading...'
                    : 'Download server.jar'}
              </button>
            </div>

            <div class="provision-step">
              <div class="provision-step-top">
                <strong>Mojang EULA</strong>
                <span class="status-pill" class:running={Boolean(setupStatus?.eula_accepted)}>
                  {setupStatus?.eula_accepted ? 'ACCEPTED' : 'UNACCEPTED'}
                </span>
              </div>
              <p class="field-hint">
                Review the <a class="provision-link" href="https://www.minecraft.net/eula" target="_blank" rel="noreferrer">official Mojang EULA</a> before writing <code>eula=true</code>.
              </p>
              {#if !setupStatus?.eula_accepted}
                <label class="eula-checkbox-row">
                  <input
                    class="eula-confirm-checkbox"
                    type="checkbox"
                    bind:checked={eulaConfirmedCheckbox}
                    disabled={!settings.instance_name || provisionBusy}
                  />
                  <span>I have read and agree to the Mojang EULA</span>
                </label>
                <button
                  type="button"
                  class="save-button small accept-eula-btn"
                  disabled={!settings.instance_name || !eulaConfirmedCheckbox || provisionBusy}
                  on:click={() => void confirmMojangEula()}
                >
                  Accept EULA
                </button>
              {/if}
            </div>
          </div>

          {#if provisionMessage}
            <p class="catalog-message" role="status">{provisionMessage}</p>
          {/if}
          {#if provisionError}
            <p class="catalog-message error" role="alert">{provisionError}</p>
          {/if}

          {#if setupStatus?.latest_crash_report}
            <details class="crash-report-box" open>
              <summary>
                <span class="crash-badge">CRASH REPORT</span>
                <strong>{setupStatus.latest_crash_report.file_name}</strong>
              </summary>
              <pre class="crash-report-pre">{setupStatus.latest_crash_report.content}</pre>
            </details>
          {/if}
        </section>

        <section class="server-status" aria-labelledby="server-status-title">
          <div>
            <span class="status-kicker">SERVER PROCESS</span>
            <h2 id="server-status-title">{settings.instance_name ?? 'No instance selected'}</h2>
            <p>{serverState === 'running'
              ? 'The server process is running. Stop it before changing server files.'
              : serverState === 'crashed'
                ? 'The server process crashed unexpectedly. Review the console logs below.'
                : 'Start requires server.jar and an existing eula=true entry in the instance directory.'}</p>
          </div>
          <span class:running={serverState === 'running'} class:crashed={serverState === 'crashed'} class="status-pill">{serverState.toUpperCase()}</span>
        </section>

        <div class="server-actions">
          <button class="save-button" type="button" disabled={serverBusy || !settings.instance_name} on:click={() => toggleServer()}>
            {serverBusy ? 'Working...' : serverState === 'running' ? 'Stop server' : serverState === 'crashed' ? 'Restart server' : 'Start server'}
            <span aria-hidden="true">&#8594;</span>
          </button>
        </div>

        {#if serverError}<p class="catalog-message error" role="alert">{serverError}</p>{/if}

        <div class="form-footer">
          <button class="save-button" type="submit" disabled={saving}>
            {saving ? 'Saving...' : 'Save server settings'}
            <span aria-hidden="true">&#8594;</span>
          </button>
          {#if saved}<p class="saved-message" role="status">Preferences saved on this device.</p>{/if}
        </div>
        </form>
        </div>
        {/if}

        {#if activeTab === 'monitor'}
        <div id="panel-monitor" class="tab-panel" role="tabpanel" aria-labelledby="tab-monitor">
          <div class="panel-heading">
            <h2>Server monitor & telemetry</h2>
            <p>Real-time resource performance, ticks, uptime, and live console stream.</p>
          </div>

          <section class="monitor-status-header" aria-labelledby="monitor-instance-title">
            <div class="monitor-server-info">
              <span class="status-kicker">MONITORED PROCESS</span>
              <h3 id="monitor-instance-title">{settings.instance_name ?? createdInstance?.name ?? 'No instance selected'}</h3>
              <span class="status-pill" class:running={serverState === 'running'} class:crashed={serverState === 'crashed'}>
                {serverState.toUpperCase()}
              </span>
            </div>
            <div class="monitor-controls">
              <button
                class="save-button small"
                type="button"
                disabled={serverBusy || !settings.instance_name}
                on:click={() => toggleServer()}
              >
                {serverBusy ? 'Working...' : serverState === 'running' ? 'Stop server' : serverState === 'crashed' ? 'Restart server' : 'Start server'}
              </button>
              <button
                class="secondary-button small"
                type="button"
                disabled={serverBusy || !settings.instance_name}
                on:click={() => {
                  void refreshServerStatus();
                  void refreshResourceMetrics();
                }}
              >
                Refresh status
              </button>
            </div>
          </section>

          {#if resourceMetrics?.low_tps_alert}
            <div class="metrics-alert" role="alert">
              <strong>
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="inline-alert-svg" aria-hidden="true">
                  <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
                  <line x1="12" y1="9" x2="12" y2="13"/>
                  <line x1="12" y1="17" x2="12.01" y2="17"/>
                </svg>
                Low TPS Warning:
              </strong> TPS has stayed below 15 for more than 10 seconds. Check console logs and mods.
            </div>
          {/if}

          <!-- Live Console Section in Monitor Tab (Positioned at Top) -->
          <section class="monitor-console-card" aria-labelledby="monitor-console-title">
            <div class="monitor-console-header">
              <div class="console-title-group">
                <h3 id="monitor-console-title">Live server console</h3>
                <span class="console-count-pill">{consoleEvents.length} lines</span>
              </div>
              <div class="console-tools">
                <button
                  type="button"
                  class="secondary-button small"
                  disabled={consoleEvents.length === 0}
                  on:click={clearConsole}
                >
                  Clear logs
                </button>
              </div>
            </div>

            <div class="monitor-terminal" role="region" aria-label="Console logs">
              {#if consoleEvents.length === 0}
                <div class="terminal-empty">
                  <p>No console messages received yet.</p>
                  <small>{serverState === 'running' ? 'Waiting for Minecraft server output...' : 'Start the server to stream logs.'}</small>
                </div>
              {:else}
                <ol class="terminal-lines" aria-live="polite">
                  {#each consoleEvents as event}
                    <li class="terminal-line" class:line-error={event.level === 'Error'} class:line-warn={event.level === 'Warn'}>
                      <span class="terminal-tag" class:tag-error={event.level === 'Error'} class:tag-warn={event.level === 'Warn'}>
                        {event.level.toUpperCase()}
                      </span>
                      <span class="terminal-msg">{event.message}</span>
                    </li>
                  {/each}
                </ol>
              {/if}
            </div>

            <form class="monitor-cmd-row" on:submit|preventDefault={submitServerCommand}>
              <input
                type="text"
                bind:value={serverCommand}
                disabled={serverState !== 'running' || serverBusy}
                placeholder={serverState === 'running' ? 'Enter Minecraft command (e.g. say Hello, help, list)...' : 'Server is stopped (cannot send commands)'}
                autocomplete="off"
                aria-label="Server console command"
              />
              <button
                type="submit"
                class="save-button small"
                disabled={serverState !== 'running' || serverBusy || !serverCommand.trim()}
              >
                Send
              </button>
            </form>
          </section>

          <!-- Resource Telemetry Section Divider -->
          <div class="monitor-section-divider" role="separator" aria-label="Resource Telemetry divider">
            <div class="divider-label">
              <svg class="divider-icon" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M22 12h-4l-3 9L9 3l-3 9H2"/>
              </svg>
              <h3 id="telemetry-title">Resource Telemetry</h3>
            </div>
            <div class="divider-line" aria-hidden="true"></div>
            <span class="telemetry-live-badge">
              <span class="live-pulse-dot" aria-hidden="true"></span>
              Live Telemetry
            </span>
          </div>

          <section class="telemetry-grid" aria-labelledby="telemetry-title">

            <!-- Card 1: Server Ticks & TPS -->
            <div class="telemetry-card telemetry-graph-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">Server Ticks (TPS)</span>
                <span class="telemetry-badge" class:healthy={!resourceMetrics || (resourceMetrics.tps ?? 20) >= 18} class:warning={resourceMetrics && (resourceMetrics.tps ?? 20) < 18}>
                  Target: 20.00
                </span>
              </div>
              <div class="telemetry-value">
                {resourceMetrics?.tps !== null && resourceMetrics?.tps !== undefined ? resourceMetrics.tps.toFixed(2) : '—'}
                <span class="telemetry-unit">TPS</span>
              </div>

              <!-- Functioning Real-time TPS Graph -->
              <div class="telemetry-graph-box">
                <svg class="telemetry-svg" viewBox="0 0 280 65" preserveAspectRatio="none" aria-label="TPS Timeline Graph">
                  <defs>
                    <linearGradient id="tps-spark-grad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stop-color="#10b981" stop-opacity="0.35" />
                      <stop offset="100%" stop-color="#10b981" stop-opacity="0.0" />
                    </linearGradient>
                  </defs>
                  <line x1="0" y1="4" x2="280" y2="4" stroke="rgba(16, 185, 129, 0.35)" stroke-dasharray="3 3" stroke-width="1" />
                  <line x1="0" y1="18" x2="280" y2="18" stroke="rgba(239, 68, 68, 0.45)" stroke-dasharray="3 3" stroke-width="1" />
                  <path d={tpsSpark.areaPath} fill="url(#tps-spark-grad)" />
                  <path d={tpsSpark.linePath} fill="none" stroke="#10b981" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                  <circle cx={tpsSpark.lastX} cy={tpsSpark.lastY} r="3.5" fill="#ffffff" stroke="#10b981" stroke-width="1.5" />
                </svg>
                <div class="graph-legend">
                  <span>Min: <strong>{tpsSpark.min.toFixed(1)}</strong></span>
                  <span class="legend-target">Target: 20.0</span>
                  <span>Avg: <strong>{tpsSpark.avg.toFixed(1)}</strong></span>
                </div>
              </div>

              <div class="telemetry-meter">
                <div
                  class="telemetry-bar tps-bar"
                  style="width: {Math.min(100, ((resourceMetrics?.tps ?? (serverState === 'running' ? 20 : 0)) / 20) * 100)}%"
                ></div>
              </div>
              <div class="telemetry-subtext">
                Tick loop rate · {resourceMetrics?.tps && resourceMetrics.tps >= 19.5 ? 'Nominal rate' : serverState === 'running' ? 'Synchronizing' : 'Server stopped'}
              </div>
            </div>

            <!-- Card 2: Process ID -->
            <div class="telemetry-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">Process ID</span>
                <span class="telemetry-badge">PID</span>
              </div>
              <div class="telemetry-value">
                {resourceMetrics?.pid ? `#${resourceMetrics.pid}` : '—'}
              </div>
              <div class="telemetry-subtext">
                {serverState === 'running' ? 'Active background process' : 'Process not running'}
              </div>
            </div>

            <!-- Card 3: Uptime -->
            <div class="telemetry-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">Process Uptime</span>
                <span class="telemetry-badge">Clock</span>
              </div>
              <div class="telemetry-value">
                {serverState === 'running' ? formatUptime(resourceMetrics?.uptime_seconds) : 'Offline'}
              </div>
              <div class="telemetry-subtext">
                {serverState === 'running' ? 'Elapsed since startup' : 'Server is not active'}
              </div>
            </div>

            <!-- Card 4: CPU Usage & Ticks -->
            <div class="telemetry-card telemetry-graph-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">CPU Usage</span>
                <span class="telemetry-badge">
                  {resourceMetrics?.cpu_ticks ? `${resourceMetrics.cpu_ticks.toLocaleString()} ticks` : '0 ticks'}
                </span>
              </div>
              <div class="telemetry-value">
                {resourceMetrics?.cpu_percent !== undefined ? resourceMetrics.cpu_percent.toFixed(1) : '0.0'}
                <span class="telemetry-unit">%</span>
              </div>

              <!-- Functioning Real-time CPU Graph -->
              <div class="telemetry-graph-box">
                <svg class="telemetry-svg" viewBox="0 0 280 65" preserveAspectRatio="none" aria-label="CPU Timeline Graph">
                  <defs>
                    <linearGradient id="cpu-spark-grad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stop-color="#38bdf8" stop-opacity="0.35" />
                      <stop offset="100%" stop-color="#38bdf8" stop-opacity="0.0" />
                    </linearGradient>
                  </defs>
                  <line x1="0" y1="34" x2="280" y2="34" stroke="rgba(56, 189, 248, 0.25)" stroke-dasharray="3 3" stroke-width="1" />
                  <path d={cpuSpark.areaPath} fill="url(#cpu-spark-grad)" />
                  <path d={cpuSpark.linePath} fill="none" stroke="#38bdf8" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                  <circle cx={cpuSpark.lastX} cy={cpuSpark.lastY} r="3.5" fill="#ffffff" stroke="#38bdf8" stroke-width="1.5" />
                </svg>
                <div class="graph-legend">
                  <span>Min: <strong>{cpuSpark.min.toFixed(1)}%</strong></span>
                  <span class="legend-target">Peak: {cpuSpark.max.toFixed(1)}%</span>
                  <span>Avg: <strong>{cpuSpark.avg.toFixed(1)}%</strong></span>
                </div>
              </div>

              <div class="telemetry-meter">
                <div
                  class="telemetry-bar cpu-bar"
                  style="width: {Math.min(100, resourceMetrics?.cpu_percent ?? 0)}%"
                ></div>
              </div>
              <div class="telemetry-subtext">
                Processor workload across threads
              </div>
            </div>

            <!-- Card 5: RAM Usage (RSS) -->
            <div class="telemetry-card telemetry-graph-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">Process RAM</span>
                <span class="telemetry-badge">RSS</span>
              </div>
              <div class="telemetry-value">
                {resourceMetrics?.memory_mb ?? 0}
                <span class="telemetry-unit">MiB</span>
              </div>

              <!-- Functioning Real-time Process RAM Graph -->
              <div class="telemetry-graph-box">
                <svg class="telemetry-svg" viewBox="0 0 280 65" preserveAspectRatio="none" aria-label="Process RAM Timeline Graph">
                  <defs>
                    <linearGradient id="ram-spark-grad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stop-color="#a855f7" stop-opacity="0.35" />
                      <stop offset="100%" stop-color="#a855f7" stop-opacity="0.0" />
                    </linearGradient>
                  </defs>
                  <path d={ramSpark.areaPath} fill="url(#ram-spark-grad)" />
                  <path d={ramSpark.linePath} fill="none" stroke="#a855f7" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                  <circle cx={ramSpark.lastX} cy={ramSpark.lastY} r="3.5" fill="#ffffff" stroke="#a855f7" stroke-width="1.5" />
                </svg>
                <div class="graph-legend">
                  <span>Min: <strong>{Math.round(ramSpark.min)} MiB</strong></span>
                  <span class="legend-target">Peak: {Math.round(ramSpark.max)} MiB</span>
                  <span>Avg: <strong>{Math.round(ramSpark.avg)} MiB</strong></span>
                </div>
              </div>

              <div class="telemetry-meter">
                <div
                  class="telemetry-bar ram-bar"
                  style="width: {Math.min(100, ((resourceMetrics?.memory_mb ?? 0) / Math.max(1, jvmSettings.ramMb)) * 100)}%"
                ></div>
              </div>
              <div class="telemetry-subtext">
                Resident set memory size in OS
              </div>
            </div>

            <!-- Card 6: JVM Heap -->
            <div class="telemetry-card telemetry-graph-card">
              <div class="telemetry-card-header">
                <span class="telemetry-label">JVM Heap Usage</span>
                <span class="telemetry-badge">JVM</span>
              </div>
              <div class="telemetry-value">
                {resourceMetrics?.heap_used_mb ?? 0}
                <span class="telemetry-unit">/ {resourceMetrics?.heap_max_mb ?? jvmSettings.ramMb} MiB</span>
              </div>

              <!-- Functioning Real-time JVM Heap Graph -->
              <div class="telemetry-graph-box">
                <svg class="telemetry-svg" viewBox="0 0 280 65" preserveAspectRatio="none" aria-label="JVM Heap Timeline Graph">
                  <defs>
                    <linearGradient id="heap-spark-grad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stop-color="#f59e0b" stop-opacity="0.35" />
                      <stop offset="100%" stop-color="#f59e0b" stop-opacity="0.0" />
                    </linearGradient>
                  </defs>
                  <path d={heapSpark.areaPath} fill="url(#heap-spark-grad)" />
                  <path d={heapSpark.linePath} fill="none" stroke="#f59e0b" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                  <circle cx={heapSpark.lastX} cy={heapSpark.lastY} r="3.5" fill="#ffffff" stroke="#f59e0b" stroke-width="1.5" />
                </svg>
                <div class="graph-legend">
                  <span>Min: <strong>{Math.round(heapSpark.min)} MiB</strong></span>
                  <span class="legend-target">Peak: {Math.round(heapSpark.max)} MiB</span>
                  <span>Avg: <strong>{Math.round(heapSpark.avg)} MiB</strong></span>
                </div>
              </div>

              <div class="telemetry-meter">
                <div
                  class="telemetry-bar heap-bar"
                  style="width: {resourceMetrics?.heap_max_mb ? Math.min(100, (resourceMetrics.heap_used_mb / resourceMetrics.heap_max_mb) * 100) : 0}%"
                ></div>
              </div>
              <div class="telemetry-subtext">
                Minecraft Java runtime heap memory
              </div>
            </div>

            <!-- Card 7: RAM Allocation -->
            <div class="telemetry-card" class:warning-card={jvmSettings.ramMb > memoryMb * 0.8}>
              <div class="telemetry-card-header">
                <span class="telemetry-label">RAM Allocation</span>
                <span class="telemetry-badge">-Xmx</span>
              </div>
              <div class="telemetry-value">
                {jvmSettings.ramMb}
                <span class="telemetry-unit">MiB</span>
              </div>
              <div class="telemetry-meter">
                <div
                  class="telemetry-bar alloc-bar"
                  class:danger={jvmSettings.ramMb > memoryMb * 0.8}
                  style="width: {Math.min(100, (jvmSettings.ramMb / Math.max(1, memoryMb)) * 100)}%"
                ></div>
              </div>
              <div class="telemetry-subtext">
                {#if jvmSettings.ramMb > memoryMb * 0.8}
                  <span class="text-danger">
                    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="inline-alert-svg" aria-hidden="true">
                      <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
                      <line x1="12" y1="9" x2="12" y2="13"/>
                      <line x1="12" y1="17" x2="12.01" y2="17"/>
                    </svg>
                    Exceeds 80% system memory ({memoryMb} MiB)
                  </span>
                {:else}
                  Configured maximum of {memoryMb} MiB detected
                {/if}
              </div>
            </div>
          </section>
        </div>
        {/if}

        {#if activeTab === 'installed'}
        <div id="panel-installed" class="tab-panel" role="tabpanel" aria-labelledby="tab-installed">
          <div class="panel-heading">
            <h2>Installed mods</h2>
            <p>{settings.instance_name ? `Managed for ${settings.instance_name}.` : 'Select an instance to manage its mods.'}</p>
          </div>
        <fieldset class="installed-mods">
          <legend>Installed mods</legend>
          <div class="installed-mods-heading">
            <p class="field-hint">
              {settings.instance_name ?? createdInstance?.name
                ? `Mods in ${settings.instance_name ?? createdInstance?.name}`
                : 'Create an instance to manage installed mods.'}
            </p>
            {#if settings.instance_name || createdInstance}
              <button
                class="refresh-mods-button"
                type="button"
                aria-label="Refresh installed mods"
                title="Refresh installed mods"
                disabled={installedModsLoading || modManagementBusy !== ''}
                on:click={() => refreshInstalledMods()}
              >
                &#8635;
              </button>
            {/if}
          </div>
          {#if installedModsPreview}
            <p class="preview-note">Browser preview stores mod status only; it does not manage files.</p>
          {/if}
          {#if installedModsLoading}
            <p class="catalog-message" role="status">Loading installed mods...</p>
          {:else if installedMods.length > 0}
            <ul class="installed-mod-list">
              {#each installedMods as mod (mod.file_name)}
                <li class="installed-mod-row">
                  <div class="installed-mod-name">
                    <strong>{mod.file_name}</strong>
                    <span>{mod.enabled ? 'Enabled' : 'Disabled'}</span>
                  </div>
                  <button
                    class="installed-action"
                    type="button"
                    disabled={modManagementBusy !== ''}
                    on:click={() => toggleInstalledMod(mod)}
                  >
                    {mod.enabled ? 'Disable' : 'Enable'}
                  </button>
                  <button
                    class="installed-action remove-action"
                    type="button"
                    disabled={modManagementBusy !== ''}
                    on:click={() => removeInstalledMod(mod)}
                  >
                    Remove
                  </button>
                </li>
              {/each}
            </ul>
          {:else if settings.instance_name || createdInstance}
            <p class="catalog-message">No mods installed.</p>
          {/if}
          {#if modManagementError}
            <p class="catalog-message error" role="alert">Mod management failed: {modManagementError}</p>
          {/if}
        </fieldset>
        {#if modUpdates.length > 0}
          <section class="updates-panel" aria-labelledby="updates-heading">
            <div class="updates-heading">
              <h3 id="updates-heading">Available updates</h3>
              <span class="update-count large" aria-label={`${modUpdates.length} updates available`}>{modUpdates.length}</span>
            </div>
            <ul class="updates-list">
              {#each modUpdates as update (update.project_id)}
                <li>
                  <div>
                    <strong>{update.file_name}</strong>
                    <span>Version {update.latest_version_number} available</span>
                  </div>
                  <button
                    class="update-mod-button"
                    type="button"
                    disabled={updateBusy !== ''}
                    on:click={() => applyAvailableUpdate(update)}
                  >
                    {updateBusy === update.project_id ? 'Updating...' : 'Update'}
                  </button>
                </li>
              {/each}
            </ul>
            <p class="field-hint">Updates are never applied without your confirmation.</p>
          </section>
        {/if}
        </div>
        {/if}

        {#if activeTab === 'catalog'}
        <div id="panel-catalog" class="tab-panel catalog-expanded-panel" role="tabpanel" aria-labelledby="tab-catalog">
          <div class="panel-heading">
            <div class="catalog-title-group">
              <h2>Mods & Content Catalog</h2>
              <p>Discover mods, resource packs, data packs, and plugins for {settings.instance_name ?? 'your server'}.</p>
            </div>
            <div class="catalog-active-meta">
              <span class="active-loader-chip" class:plugin-capable={supportsPlugins}>
                Loader: <strong>{loader}</strong> ({minecraftVersion})
                {#if supportsPlugins}
                  <span class="loader-plugin-tag">Plugins supported</span>
                {/if}
              </span>
            </div>
          </div>

        <fieldset class="mod-catalog theme-{modCatalogSource}">
          <legend>Mod catalog</legend>

          <div class="catalog-workbench-layout">
            <!-- Left: Authentic Collapsible Category Sidebar Drawer (Image 3) -->
            <aside class="catalog-category-sidebar" class:collapsed={isCategoryCollapsed} class:key-locked={isCurseForgeKeyRequired} aria-label="Catalog categories">
              <button
                type="button"
                class="category-sidebar-header-btn"
                title={isCurseForgeKeyRequired ? 'CurseForge API key required to browse categories' : isCategoryCollapsed ? 'Expand categories' : 'Collapse categories'}
                aria-label={isCategoryCollapsed ? 'Expand categories' : 'Collapse categories'}
                on:click={() => (isCategoryCollapsed = !isCategoryCollapsed)}
              >
                <div class="category-header-title-wrap">
                  <span class="category-header-title">Category</span>
                  {#if isCurseForgeKeyRequired}
                    <span class="key-locked-badge" title="API Key Required">Key required</span>
                  {/if}
                </div>
                <span class="category-collapse-arrow" aria-hidden="true">
                  <svg class="collapse-chevron" class:rotated={isCategoryCollapsed} viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="18 15 12 9 6 15"/>
                  </svg>
                </span>
              </button>

              {#if !isCategoryCollapsed}
                <div class="category-chip-bar category-drawer-list" role="group" aria-label="Mod categories">
                  {#each authenticCategories as cat (cat.id)}
                    <button
                      type="button"
                      class="category-chip category-drawer-item"
                      class:active={modCatalogCategory.toLowerCase() === cat.id.toLowerCase() || (cat.id === 'all' && modCatalogCategory === 'all')}
                      disabled={isCurseForgeKeyRequired}
                      title={isCurseForgeKeyRequired ? 'CurseForge API key required to filter categories' : `Filter by ${cat.label}`}
                      on:click={() => {
                        if (!isCurseForgeKeyRequired) {
                          modCatalogCategory = cat.id;
                          scheduleModSearch();
                        }
                      }}
                    >
                      <span class="chip-icon" aria-hidden="true">
                        <svg class="category-svg" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          {@html categorySvgPaths[cat.id] ?? categorySvgPaths.all}
                        </svg>
                      </span>
                      <span class="chip-label">{cat.label}</span>
                      {#if (modCatalogCategory.toLowerCase() === cat.id.toLowerCase() || (cat.id === 'all' && modCatalogCategory === 'all'))}
                        <span class="chip-active-check" aria-hidden="true">
                          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="20 6 9 17 4 12"/>
                          </svg>
                        </span>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </aside>

            <!-- Right: Spacious Main Catalog Body -->
            <div class="catalog-body-area">
              <div class="catalog-controls-deck">
                <div class="catalog-controls-top-row">
                  <!-- Source Selector with Official SVGs -->
                  <div class="catalog-source-selector" role="group" aria-label="Mod catalog source">
                  <button
                    type="button"
                    class="catalog-source-btn source-modrinth"
                    class:chosen={modCatalogSource === 'modrinth'}
                    on:click={() => {
                      modCatalogSource = 'modrinth';
                      modCatalogCategory = 'all';
                      scheduleModSearch();
                    }}
                  >
                    <svg class="official-logo modrinth-logo" xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" aria-hidden="true">
                      <title>modrinth</title>
                      <path fill="currentColor" d="M12.25 0a11.78 11.77 0 0 0-8.92 3.73a11 11 0 0 0-2.17 3.11a11.37 11.36 0 0 0-1.16 5.17c0 1.42.17 2.5.6 3.77c.24.76.77 1.9 1.17 2.53a12.3 12.3 0 0 0 8.85 5.64c.44.05 2.54.07 2.76.02c.2-.04.22.1-.26-1.7l-.36-1.37l-1.01-.06a8.5 8.49 0 0 1-5.18-1.8a5.3 5.3 0 0 1-1.3-1.26c0-.05.34-.28.74-.5a37.57 37.55 0 0 1 2.88-1.63c.03 0 .5.45 1.06.98l1 .97l2.07-.43l2.06-.43l1.47-1.47c.8-.8 1.48-1.5 1.48-1.52c0-.09-.42-1.63-.46-1.7c-.04-.06-.2-.03-1.02.18c-.53.13-1.2.3-1.45.4l-.48.15l-.53.53l-.53.53l-.93.1l-.93.07l-.52-.5a2.7 2.7 0 0 1-.96-1.7l-.13-.6l.43-.57c.68-.9.68-.9 1.46-1.1c.4-.1.65-.2.83-.33c.13-.1.65-.58 1.14-1.07l.9-.9l-.7-.7l-.7-.7l-1.95.54c-1.07.3-1.96.53-1.97.53c-.03 0-2.23 2.48-2.63 2.97l-.29.35l.28 1.03c.16.56.3 1.16.31 1.34l.03.3l-.34.23c-.37.23-2.22 1.3-2.84 1.63c-.36.2-.37.2-.44.1c-.08-.1-.23-.6-.32-1.03c-.18-.86-.17-2.75.02-3.73a8.84 8.84 0 0 1 7.9-6.93c.43-.03.77-.08.78-.1c.06-.17.5-3 .47-3.04c-.01-.02-.1-.02-.2-.03Zm3.68.67c-.2 0-.3.1-.37.38c-.06.23-.46 2.42-.46 2.52c0 .04.1.11.22.16a8.51 8.5 0 0 1 2.99 2a8.38 8.38 0 0 1 2.16 3.45a6.9 6.9 0 0 1 .4 2.8c0 1.07 0 1.27-.1 1.73a9.4 9.4 0 0 1-1.76 3.77c-.32.4-.98 1.06-1.37 1.38c-.38.32-1.54 1.1-1.7 1.14c-.1.03-.1.06-.07.26c.03.18.64 2.56.7 2.78l.06.06a12.07 12.06 0 0 0 7.27-9.4c.13-.77.13-2.58 0-3.4a11.96 11.95 0 0 0-5.73-8.58c-.7-.42-2.05-1.06-2.25-1.06Z"/>
                    </svg>
                    <span>Modrinth</span>
                  </button>
                  <button
                    type="button"
                    class="catalog-source-btn source-curseforge"
                    class:chosen={modCatalogSource === 'curseforge'}
                    on:click={() => {
                      modCatalogSource = 'curseforge';
                      modCatalogCategory = 'all';
                      scheduleModSearch();
                    }}
                  >
                    <svg class="official-logo curseforge-logo" xmlns="http://www.w3.org/2000/svg" width="18" height="18" fill="none" viewBox="0 0 260 256" aria-hidden="true">
                      <path fill="currentColor" d="M196.422 98.77S247.874 90.623 256 66.858h-78.819V48H4l21.334 24.862v25.473s53.83-2.811 74.653 13.047c28.502 26.532-32.058 62.397-32.058 62.397l-10.384 34.512c16.239-15.529 47.188-35.618 103.933-34.65-21.594 6.854-43.307 17.56-60.211 34.65h114.71l-10.802-34.512s-83.139-49.235-8.753-75.005v-.004Z"/>
                    </svg>
                    <span>CurseForge</span>
                  </button>
                </div>

                <!-- Project Type Tabs: Mods, Resource Packs, Data Packs, Plugins -->
                <div class="project-type-selector" class:key-locked={isCurseForgeKeyRequired} role="tablist" aria-label="Project types">
                  <button
                    type="button"
                    class="project-type-pill"
                    class:active={catalogProjectType === 'mod'}
                    class:key-locked={isCurseForgeKeyRequired}
                    disabled={isCurseForgeKeyRequired}
                    title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'Mods'}
                    role="tab"
                    aria-selected={catalogProjectType === 'mod'}
                    on:click={() => {
                      if (!isCurseForgeKeyRequired) {
                        catalogProjectType = 'mod';
                        scheduleModSearch();
                      }
                    }}
                  >
                    <svg class="pill-svg" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="m7.5 4.27 9 5.15"/>
                      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/>
                      <path d="m3.3 7 8.7 5 8.7-5"/>
                      <path d="M12 22V12"/>
                    </svg>
                    <span>Mods</span>
                  </button>
                  <button
                    type="button"
                    class="project-type-pill"
                    class:active={catalogProjectType === 'resourcepack'}
                    class:key-locked={isCurseForgeKeyRequired}
                    disabled={isCurseForgeKeyRequired}
                    title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'Resource Packs'}
                    role="tab"
                    aria-selected={catalogProjectType === 'resourcepack'}
                    on:click={() => {
                      if (!isCurseForgeKeyRequired) {
                        catalogProjectType = 'resourcepack';
                        scheduleModSearch();
                      }
                    }}
                  >
                    <svg class="pill-svg" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/>
                      <circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/>
                      <circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/>
                      <circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/>
                      <path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.563-2.512 5.563-5.563C22 6.5 17.5 2 12 2Z"/>
                    </svg>
                    <span>Resource Packs</span>
                  </button>
                  <button
                    type="button"
                    class="project-type-pill"
                    class:active={catalogProjectType === 'datapack'}
                    class:key-locked={isCurseForgeKeyRequired}
                    disabled={isCurseForgeKeyRequired}
                    title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'Data Packs'}
                    role="tab"
                    aria-selected={catalogProjectType === 'datapack'}
                    on:click={() => {
                      if (!isCurseForgeKeyRequired) {
                        catalogProjectType = 'datapack';
                        scheduleModSearch();
                      }
                    }}
                  >
                    <svg class="pill-svg" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <ellipse cx="12" cy="5" rx="9" ry="3"/>
                      <path d="M3 5V19A9 3 0 0 0 21 19V5"/>
                      <path d="M3 12A9 3 0 0 0 21 12"/>
                    </svg>
                    <span>Data Packs</span>
                  </button>
                  <button
                    type="button"
                    class="project-type-pill"
                    class:active={catalogProjectType === 'plugin'}
                    class:locked={!supportsPlugins || isCurseForgeKeyRequired}
                    role="tab"
                    aria-selected={catalogProjectType === 'plugin'}
                    disabled={!supportsPlugins || isCurseForgeKeyRequired}
                    title={isCurseForgeKeyRequired ? 'CurseForge API key required' : supportsPlugins ? 'Plugins (Paper, Spigot, Bukkit, Folia, Purpur, Sponge)' : 'Plugins require Paper, Spigot, Bukkit, Folia, Purpur, or Sponge'}
                    on:click={() => {
                      if (supportsPlugins && !isCurseForgeKeyRequired) {
                        catalogProjectType = 'plugin';
                        scheduleModSearch();
                      }
                    }}
                  >
                    <svg class="pill-svg" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <path d="M12 2v4"/>
                      <path d="m19 5-3 3"/>
                      <path d="m5 19 3-3"/>
                      <path d="m19 19-3-3"/>
                      <path d="m5 5 3 3"/>
                      <path d="M12 18v4"/>
                      <path d="M2 12h4"/>
                      <path d="M18 12h4"/>
                    </svg>
                    <span>Plugins</span>
                    {#if !supportsPlugins}
                      <span class="locked-loader-badge">
                        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                          <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
                          <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                        </svg>
                        Paper/Spigot only
                      </span>
                    {/if}
                  </button>
                </div>
              </div>

                <!-- Environment Compatibility Filter (Centered, All / Client / Server) -->
                <div class="env-toggle-container">
                  <div class="env-toggle-group" class:key-locked={isCurseForgeKeyRequired} role="group" aria-label="Environment compatibility">
                    <button
                      type="button"
                      class="env-toggle-btn"
                      class:active={catalogEnvironment === 'all'}
                      class:key-locked={isCurseForgeKeyRequired}
                      disabled={isCurseForgeKeyRequired}
                      title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'All environments'}
                      on:click={() => {
                        if (!isCurseForgeKeyRequired) {
                          catalogEnvironment = 'all';
                          scheduleModSearch();
                        }
                      }}
                    >All</button>
                    <button
                      type="button"
                      class="env-toggle-btn"
                      class:active={catalogEnvironment === 'client'}
                      class:key-locked={isCurseForgeKeyRequired}
                      disabled={isCurseForgeKeyRequired}
                      title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'Client only'}
                      on:click={() => {
                        if (!isCurseForgeKeyRequired) {
                          catalogEnvironment = 'client';
                          scheduleModSearch();
                        }
                      }}
                    >Client</button>
                    <button
                      type="button"
                      class="env-toggle-btn"
                      class:active={catalogEnvironment === 'server'}
                      class:key-locked={isCurseForgeKeyRequired}
                      disabled={isCurseForgeKeyRequired}
                      title={isCurseForgeKeyRequired ? 'CurseForge API key required' : 'Server only'}
                      on:click={() => {
                        if (!isCurseForgeKeyRequired) {
                          catalogEnvironment = 'server';
                          scheduleModSearch();
                        }
                      }}
                    >Server</button>
                  </div>
                </div>
              </div>

              {#if modCatalogSource === 'curseforge'}
                <div class="curseforge-key-section">
                  <label class="form-field">
                    <span>CurseForge API Key (OS Keychain)</span>
                    <p class="field-hint">Required for CurseForge API requests. Saved directly to your device's OS keychain, never in configs or logs.</p>
                    <div class="password-input-row">
                      <input
                        type={curseforgeKeyMasked ? 'password' : 'text'}
                        value={curseforgeApiKey}
                        on:input={(e) => (curseforgeApiKey = e.currentTarget.value)}
                        placeholder="Enter CurseForge Core API key"
                        autocomplete="off"
                      />
                      <button
                        type="button"
                        class="secondary-button small"
                        on:click={() => (curseforgeKeyMasked = !curseforgeKeyMasked)}
                      >
                        {curseforgeKeyMasked ? 'Show' : 'Hide'}
                      </button>
                      <button
                        type="button"
                        class="save-button small save-curseforge-key-btn"
                        disabled={!curseforgeApiKey.trim() || curseforgeKeyBusy}
                        on:click={() => void saveCurseForgeKey()}
                      >
                        {curseforgeKeyBusy ? 'Saving...' : 'Save to Keychain'}
                      </button>
                    </div>
                  </label>
                  {#if curseforgeKeyMessage}
                    <p class="catalog-message" role="status">{curseforgeKeyMessage}</p>
                  {/if}
                  {#if curseforgeKeyError}
                    <p class="catalog-message error" role="alert">{curseforgeKeyError}</p>
                  {/if}

                  <!-- Explanatory hint on why the API key is required (shown when not set up yet) -->
                  {#if !curseforgeApiKeySaved}
                    <div class="cf-key-hint-box" role="note" aria-label="Why a CurseForge API key is required">
                      <div class="cf-key-hint-header">
                        <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="hint-svg" aria-hidden="true">
                          <circle cx="12" cy="12" r="10"/>
                          <line x1="12" y1="16" x2="12" y2="12"/>
                          <line x1="12" y1="8" x2="12.01" y2="8"/>
                        </svg>
                        <strong>Why is an API key required for CurseForge?</strong>
                      </div>
                      <p class="cf-key-hint-desc">
                        Unlike open platforms (such as Modrinth) that allow public unauthenticated queries, CurseForge (Overwolf) requires an authenticated Core API token for all catalog queries, category filters, and mod downloads. This requirement enforces creator distribution rights and prevents unauthorized scraping.
                      </p>
                      <div class="cf-key-hint-steps">
                        <span class="steps-title">How to get a free API key:</span>
                        <ol>
                          <li>Open the <a href="https://support.curseforge.com/en/support/solutions/articles/9000207877-how-to-generate-an-api-key" target="_blank" rel="noreferrer">CurseForge for Studios console &rarr;</a></li>
                          <li>Generate or copy your free Core API token.</li>
                          <li>Paste it into the field above and click <em>Save to Keychain</em>.</li>
                        </ol>
                      </div>
                      <div class="cf-key-security-badge">
                        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                          <rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>
                          <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                        </svg>
                        <span>Encrypted directly in your device's native OS keychain via <code>keyring</code> (never in plaintext configs or crash logs).</span>
                      </div>
                    </div>
                  {:else}
                    <div class="cf-key-active-badge">
                      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                        <polyline points="20 6 9 17 4 12"/>
                      </svg>
                      <span>CurseForge API Key is active in your OS Keychain. Catalog unlocked.</span>
                    </div>
                  {/if}
                </div>
              {/if}

              <div class="catalog-search-bar-row">
                <label class="form-field catalog-search-field">
                  <span>Search {catalogProjectType === 'mod' ? 'mods' : catalogProjectType === 'resourcepack' ? 'resource packs' : catalogProjectType === 'datapack' ? 'data packs' : 'plugins'}</span>
                  <div class="catalog-input-container">
                    <span class="search-glass-icon" aria-hidden="true">
                      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="11" cy="11" r="8"/>
                        <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                      </svg>
                    </span>
                    <input
                      value={modQuery}
                      disabled={isCurseForgeKeyRequired}
                      on:input={(event) => {
                        modQuery = event.currentTarget.value;
                        scheduleModSearch();
                      }}
                      type="search"
                      maxlength="100"
                      autocomplete="off"
                      placeholder={isCurseForgeKeyRequired
                        ? 'CurseForge API key required to search catalog...'
                        : `Search ${modCatalogSource === 'curseforge' ? 'CurseForge' : 'Modrinth'} for ${catalogProjectType === 'mod' ? 'Sodium, JEI, Lithium...' : catalogProjectType === 'resourcepack' ? 'Faithful, Bare Bones...' : catalogProjectType === 'datapack' ? 'Terralith, Incendium...' : 'EssentialsX, Vault, CoreProtect...'}`}
                    />
                    {#if modQuery}
                      <button
                        type="button"
                        class="search-clear-action-btn"
                        aria-label="Clear search query"
                        disabled={isCurseForgeKeyRequired}
                        on:click={() => {
                          modQuery = '';
                          scheduleModSearch();
                        }}
                      >
                        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                          <line x1="18" y1="6" x2="6" y2="18"/>
                          <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                      </button>
                    {/if}
                  </div>
                </label>
              </div>

              {#if isCurseForgeKeyRequired}
                <div class="curseforge-locked-banner" role="alert">
                  <div class="locked-banner-icon" aria-hidden="true">
                    <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>
                      <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
                    </svg>
                  </div>
                  <div class="locked-banner-content">
                    <h4>CurseForge Catalog Locked</h4>
                    <p>Enter your CurseForge Core API key above and save it to your OS Keychain to search mods, browse categories, and install addons.</p>
                  </div>
                </div>
              {:else if modSearchPreview}
                <p class="preview-note">Showing sample results in browser preview. The desktop app searches {modCatalogSource === 'curseforge' ? 'CurseForge' : 'Modrinth'}.</p>
              {/if}
              {#if modSearchError}
                <p class="catalog-message error" role="alert">Search failed: {modSearchError}</p>
              {:else if modSearchLoading}
                <p class="catalog-message" role="status">Searching {modCatalogSource === 'curseforge' ? 'CurseForge' : 'Modrinth'}...</p>
              {:else if modQuery.trim().length < 2 && modCatalogCategory === 'all' && catalogProjectType === 'mod'}
                <p class="catalog-message">Enter at least 2 characters to search or select a category from the sidebar.</p>
              {:else if modResults.length === 0}
                <p class="catalog-message">No matching {catalogProjectType === 'mod' ? 'mods' : catalogProjectType === 'resourcepack' ? 'resource packs' : catalogProjectType === 'datapack' ? 'data packs' : 'plugins'} found.</p>
              {:else}
                <ul class="mod-results" aria-live="polite">
                  {#each modResults as mod (mod.project_id)}
                    <li class="mod-result">
                      {#if mod.icon_url}
                        <img class="mod-thumbnail" src={mod.icon_url} alt="" />
                      {:else}
                        <span class="mod-mark" aria-hidden="true">{mod.title.slice(0, 1)}</span>
                      {/if}
                      <div class="mod-copy">
                        <div class="mod-title-row">
                          <strong class="mod-title-text">{mod.title}</strong>
                          <span class="mod-source-badge mod-source-{mod.source ?? modCatalogSource}">
                            {#if (mod.source ?? modCatalogSource) === 'curseforge'}
                              <svg class="badge-source-svg" xmlns="http://www.w3.org/2000/svg" width="12" height="12" fill="none" viewBox="0 0 260 256" aria-hidden="true">
                                <path fill="#f16436" d="M196.422 98.77S247.874 90.623 256 66.858h-78.819V48H4l21.334 24.862v25.473s53.83-2.811 74.653 13.047c28.502 26.532-32.058 62.397-32.058 62.397l-10.384 34.512c16.239-15.529 47.188-35.618 103.933-34.65-21.594 6.854-43.307 17.56-60.211 34.65h114.71l-10.802-34.512s-83.139-49.235-8.753-75.005v-.004Z"/>
                              </svg>
                              <span>CurseForge</span>
                            {:else}
                              <svg class="badge-source-svg" xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" aria-hidden="true">
                                <title>modrinth</title>
                                <path fill="#1bd96a" d="M12.25 0a11.78 11.77 0 0 0-8.92 3.73a11 11 0 0 0-2.17 3.11a11.37 11.36 0 0 0-1.16 5.17c0 1.42.17 2.5.6 3.77c.24.76.77 1.9 1.17 2.53a12.3 12.3 0 0 0 8.85 5.64c.44.05 2.54.07 2.76.02c.2-.04.22.1-.26-1.7l-.36-1.37l-1.01-.06a8.5 8.49 0 0 1-5.18-1.8a5.3 5.3 0 0 1-1.3-1.26c0-.05.34-.28.74-.5a37.57 37.55 0 0 1 2.88-1.63c.03 0 .5.45 1.06.98l1 .97l2.07-.43l2.06-.43l1.47-1.47c.8-.8 1.48-1.5 1.48-1.52c0-.09-.42-1.63-.46-1.7c-.04-.06-.2-.03-1.02.18c-.53.13-1.2.3-1.45.4l-.48.15l-.53.53l-.53.53l-.93.1l-.93.07l-.52-.5a2.7 2.7 0 0 1-.96-1.7l-.13-.6l.43-.57c.68-.9.68-.9 1.46-1.1c.4-.1.65-.2.83-.33c.13-.1.65-.58 1.14-1.07l.9-.9l-.7-.7l-.7-.7l-1.95.54c-1.07.3-1.96.53-1.97.53c-.03 0-2.23 2.48-2.63 2.97l-.29.35l.28 1.03c.16.56.3 1.16.31 1.34l.03.3l-.34.23c-.37.23-2.22 1.3-2.84 1.63c-.36.2-.37.2-.44.1c-.08-.1-.23-.6-.32-1.03c-.18-.86-.17-2.75.02-3.73a8.84 8.84 0 0 1 7.9-6.93c.43-.03.77-.08.78-.1c.06-.17.5-3 .47-3.04c-.01-.02-.1-.02-.2-.03Zm3.68.67c-.2 0-.3.1-.37.38c-.06.23-.46 2.42-.46 2.52c0 .04.1.11.22.16a8.51 8.5 0 0 1 2.99 2a8.38 8.38 0 0 1 2.16 3.45a6.9 6.9 0 0 1 .4 2.8c0 1.07 0 1.27-.1 1.73a9.4 9.4 0 0 1-1.76 3.77c-.32.4-.98 1.06-1.37 1.38c-.38.32-1.54 1.1-1.7 1.14c-.1.03-.1.06-.07.26c.03.18.64 2.56.7 2.78l.06.06a12.07 12.06 0 0 0 7.27-9.4c.13-.77.13-2.58 0-3.4a11.96 11.95 0 0 0-5.73-8.58c-.7-.42-2.05-1.06-2.25-1.06Z"/>
                              </svg>
                              <span>Modrinth</span>
                            {/if}
                          </span>
                          {#if mod.project_type && mod.project_type !== 'mod'}
                            <span class="mod-type-pill">{mod.project_type}</span>
                          {/if}
                          {#if mod.client_side && mod.server_side}
                            <span class="mod-env-badge">
                              {mod.client_side === 'required' && mod.server_side === 'unsupported'
                                ? 'Client only'
                                : mod.client_side === 'unsupported' && mod.server_side === 'required'
                                  ? 'Server only'
                                  : 'Client & Server'}
                            </span>
                          {/if}
                        </div>
                        <div class="mod-meta-row">
                          <span class="mod-author-tag">by {mod.author}</span>
                          <span class="mod-downloads-count">
                            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
                              <polyline points="7 10 12 15 17 10"/>
                              <line x1="12" x2="12" y1="15" y2="3"/>
                            </svg>
                            {formatCompactNumber(mod.downloads)} downloads
                          </span>
                        </div>
                        <p class="mod-desc-text">{mod.description}</p>
                        {#if mod.categories && mod.categories.length > 0}
                          <div class="mod-categories-tags">
                            {#each mod.categories as cat}
                              <span class="mod-category-pill">{cat}</span>
                            {/each}
                          </div>
                        {/if}
                      </div>
                      {#if mod.distribution_disabled}
                        <a
                          class="secondary-button small external-link-btn"
                          href={mod.project_url ?? `https://www.curseforge.com/minecraft/mc-mods/${mod.slug}`}
                          target="_blank"
                          rel="noreferrer"
                        >
                          Distribution Disabled (View &nearr;)
                        </a>
                      {:else}
                        <button
                          class="install-mod-button"
                          type="button"
                          disabled={installingProjectId !== '' || mod.project_id in installedProjectFiles}
                          on:click={() => installSelectedMod(mod)}
                        >
                          {mod.project_id in installedProjectFiles
                            ? 'Installed'
                            : installingProjectId === mod.project_id
                              ? 'Installing...'
                              : 'Install'}
                        </button>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}

              {#if modInstallMessage}
                <p class="catalog-message" role="status">{modInstallMessage}</p>
              {/if}
              {#if modInstallError}
                <p class="catalog-message error" role="alert">Install failed: {modInstallError}</p>
              {/if}
            </div>
          </div>
        </fieldset>
        </div>
        {/if}

        {#if activeTab === 'files'}
        <div id="panel-files" class="tab-panel" role="tabpanel" aria-labelledby="tab-files">
          <div class="panel-heading">
            <h2>File explorer</h2>
            <p>Inspect and edit configuration files in {settings?.instance_name ?? 'this instance'}.</p>
          </div>
          <div class="explorer-toolbar">
            <div class="path-breadcrumbs">
              <button
                type="button"
                class="breadcrumb-btn"
                disabled={!currentFilePath}
                on:click={() => void loadFiles(settings?.instance_name ?? createdInstance?.name, '')}
              >/</button>
              {#if currentFilePath}
                <span class="path-display">{currentFilePath}</span>
                <button
                  type="button"
                  class="secondary-button small"
                  on:click={navigateUp}
                >Up &uarr;</button>
              {/if}
            </div>
            <button
              type="button"
              class="secondary-button small"
              disabled={filesLoading}
              on:click={() => void loadFiles()}
            >Refresh</button>
          </div>

          {#if fileMessage}
            <p class="feedback success" role="status">{fileMessage}</p>
          {/if}
          {#if filesError}
            <p class="feedback error" role="alert">{filesError}</p>
          {/if}

          <div class="explorer-layout">
            <div class="file-browser-pane">
              <h3>Files</h3>
              {#if filesLoading}
                <p class="loading-state">Loading directory contents...</p>
              {:else if fileEntries.length === 0}
                <p class="empty-state">No files or folders found.</p>
              {:else}
                <ul class="file-tree" role="list">
                  {#each fileEntries as entry}
                    <li class="file-tree-item" class:selected={selectedFile?.relative_path === entry.relative_path}>
                      <button
                        type="button"
                        class="file-select-btn"
                        on:click={() => void openFile(entry)}
                      >
                        <span class="entry-icon" aria-hidden="true">
                          {#if entry.is_dir}
                            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                            </svg>
                          {:else}
                            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                              <polyline points="14 2 14 8 20 8"/>
                            </svg>
                          {/if}
                        </span>
                        <span class="entry-name">{entry.name}</span>
                        {#if !entry.is_dir}
                          <span class="entry-size">{entry.size_bytes} B</span>
                        {/if}
                      </button>
                      <button
                        type="button"
                        class="file-delete-btn"
                        aria-label={`Delete ${entry.name}`}
                        title={`Delete ${entry.name}`}
                        on:click={() => void deleteEntry(entry)}
                      >&times;</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>

            <div class="file-editor-pane">
              {#if selectedFile}
                <div class="editor-header">
                  <h3>{selectedFile.name}</h3>
                  <button
                    type="button"
                    class="save-button"
                    disabled={fileSaveBusy || editingFileContent === selectedFileContent}
                    on:click={() => void saveOpenedFile()}
                  >
                    {fileSaveBusy ? 'Saving...' : 'Save file'}
                  </button>
                </div>
                <textarea
                  class="file-editor-area"
                  aria-label={`Contents of ${selectedFile.name}`}
                  rows="18"
                  bind:value={editingFileContent}
                ></textarea>
              {:else}
                <div class="empty-editor-prompt">
                  <p>Select a file from the list to view or edit its contents.</p>
                </div>
              {/if}
            </div>
          </div>
        </div>
        {/if}

        {#if activeTab === 'players'}
        <div id="panel-players" class="tab-panel" role="tabpanel" aria-labelledby="tab-players">
          <div class="panel-heading">
            <h2>Player management</h2>
            <p>Manage whitelist and banlist entries for {settings?.instance_name ?? 'this instance'}.</p>
          </div>

          {#if playersMessage}
            <p class="feedback success" role="status">{playersMessage}</p>
          {/if}
          {#if playersError}
            <p class="feedback error" role="alert">{playersError}</p>
          {/if}

          <div class="players-layout">
            <fieldset class="player-card">
              <legend>Whitelist ({playerLists.whitelist.length})</legend>
              <form class="player-add-row" on:submit|preventDefault={() => void addPlayerToWhitelist()}>
                <label class="form-field">
                  <span>Player name</span>
                  <input
                    type="text"
                    aria-label="Player name to whitelist"
                    placeholder="e.g. Steve"
                    bind:value={newWhitelistName}
                  />
                </label>
                <button
                  type="submit"
                  class="save-button small"
                  disabled={!newWhitelistName.trim() || playersLoading}
                >Add to whitelist</button>
              </form>

              {#if playerLists.whitelist.length === 0}
                <p class="empty-state">No whitelisted players.</p>
              {:else}
                <ul class="player-entry-list" role="list">
                  {#each playerLists.whitelist as player}
                    <li class="player-entry-row">
                      <div class="player-entry-info">
                        <strong>{player.name}</strong>
                        <small>{player.uuid}</small>
                      </div>
                      <button
                        type="button"
                        class="secondary-button small remove-player"
                        aria-label={`Remove ${player.name} from whitelist`}
                        on:click={() => void removePlayerFromWhitelist(player)}
                      >Remove</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </fieldset>

            <fieldset class="player-card">
              <legend>Banned players ({playerLists.banlist.length})</legend>
              <form class="player-add-row" on:submit|preventDefault={() => void addPlayerToBanlist()}>
                <label class="form-field">
                  <span>Player name</span>
                  <input
                    type="text"
                    aria-label="Player name to ban"
                    placeholder="e.g. Griefer123"
                    bind:value={newBanName}
                  />
                </label>
                <button
                  type="submit"
                  class="danger-button small"
                  disabled={!newBanName.trim() || playersLoading}
                >Ban player</button>
              </form>

              {#if playerLists.banlist.length === 0}
                <p class="empty-state">No banned players.</p>
              {:else}
                <ul class="player-entry-list" role="list">
                  {#each playerLists.banlist as player}
                    <li class="player-entry-row">
                      <div class="player-entry-info">
                        <strong>{player.name}</strong>
                        <small>{player.uuid}</small>
                      </div>
                      <button
                        type="button"
                        class="secondary-button small unban-player"
                        aria-label={`Unban ${player.name}`}
                        on:click={() => void removePlayerFromBanlist(player)}
                      >Unban</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </fieldset>
          </div>
        </div>
        {/if}
      </div>
    {:else}
      <p class="feedback error" role="alert">Could not load settings: {error}</p>
    {/if}

    {#if error && settings}
      <p class="feedback error" role="alert">Could not save settings: {error}</p>
    {/if}
    </section>
  </div>
</div>
</main>
{/if}

<svelte:window on:keydown={handleGlobalKeydown} on:click={() => { showOptionsDropdown = false; }} />

<!-- Preferences Modal (Image 2) -->
{#if showPreferencesModal}
  <div class="modal-backdrop" role="presentation" on:click|self={() => (showPreferencesModal = false)}>
    <div class="preferences-dialog" role="dialog" aria-modal="true" aria-labelledby="pref-modal-title">
      <div class="pref-topbar">
        <button
          type="button"
          class="pref-tool-btn"
          title="Search preferences"
          aria-label="Search preferences"
          on:click={() => (showPrefSearch = !showPrefSearch)}
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <circle cx="11" cy="11" r="8"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
        </button>

        <div class="pref-segmented-nav" role="tablist" aria-label="Preferences sections">
          <button
            type="button"
            class="pref-segment-btn"
            class:active={prefActiveTab === 'general'}
            role="tab"
            aria-selected={prefActiveTab === 'general'}
            on:click={() => (prefActiveTab = 'general')}
          >
            <span class="segment-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="3"/>
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
              </svg>
            </span>
            <span>General</span>
          </button>
          <button
            type="button"
            class="pref-segment-btn"
            class:active={prefActiveTab === 'remote'}
            role="tab"
            aria-selected={prefActiveTab === 'remote'}
            on:click={() => (prefActiveTab = 'remote')}
          >
            <span class="segment-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="10"/>
                <line x1="2" x2="22" y1="12" y2="12"/>
                <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
              </svg>
            </span>
            <span>Remote</span>
          </button>
        </div>

        <button
          type="button"
          class="pref-close-btn"
          title="Close preferences"
          aria-label="Close preferences"
          on:click={() => (showPreferencesModal = false)}
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      {#if showPrefSearch}
        <div class="pref-search-bar">
          <input
            type="search"
            bind:value={prefSearchQuery}
            placeholder="Search preferences..."
            aria-label="Search preferences"
          />
        </div>
      {/if}

      <div class="pref-body-scroll">
        {#if prefActiveTab === 'general'}
          <!-- Application Section -->
          <div class="pref-section">
            <h3 class="pref-section-heading">Application</h3>
            <div class="pref-card-group">
              <div class="pref-row pref-folder-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">Data folder</div>
                  <div class="pref-row-subtitle" title={prefDataFolder}>{prefDataFolder}</div>
                </div>
                <button
                  type="button"
                  class="pref-icon-action-btn"
                  title="Browse data folder"
                  aria-label="Browse data folder"
                  on:click={browseDataFolder}
                >
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
                  </svg>
                </button>
              </div>

              <div class="pref-row">
                <div class="pref-row-info">
                  <span class="pref-row-title">Run in background</span>
                </div>
                <label class="pref-switch" aria-label="Run in background">
                  <input type="checkbox" bind:checked={prefRunInBackground} />
                  <span class="switch-pill"></span>
                </label>
              </div>

              <div class="pref-row">
                <div class="pref-row-info">
                  <span class="pref-row-title">Open Redstone on startup</span>
                </div>
                <label class="pref-switch" aria-label="Open Redstone on startup">
                  <input type="checkbox" bind:checked={prefOpenOnStartup} />
                  <span class="switch-pill"></span>
                </label>
              </div>

              <div class="pref-row">
                <div class="pref-row-info">
                  <span class="pref-row-title">Prevent sleep while server is running</span>
                </div>
                <label class="pref-switch" aria-label="Prevent sleep while server is running">
                  <input type="checkbox" bind:checked={prefPreventSleep} />
                  <span class="switch-pill"></span>
                </label>
              </div>
            </div>
          </div>

          <!-- Appearance Section -->
          <div class="pref-section">
            <h3 class="pref-section-heading">Appearance</h3>
            <div class="pref-card-group">
              <div class="pref-row">
                <div class="pref-row-info">
                  <span class="pref-row-title">Language</span>
                </div>
                <div class="pref-select-wrapper">
                  <select bind:value={prefLanguage} class="pref-select" aria-label="Language">
                    <option value="System default">System default</option>
                    <option value="English (US)">English (US)</option>
                    <option value="Español">Español</option>
                    <option value="Français">Français</option>
                    <option value="Deutsch">Deutsch</option>
                    <option value="日本語">日本語</option>
                    <option value="简体中文">简体中文</option>
                  </select>
                </div>
              </div>
            </div>
          </div>

          <!-- Backups Section -->
          <div class="pref-section">
            <h3 class="pref-section-heading">Backups</h3>
            <div class="pref-card-group">
              <div class="pref-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">Auto backup on stop</div>
                  <div class="pref-row-subtitle">{prefBackupScope === 'worlds' ? 'Worlds only' : 'Full server'}</div>
                </div>
                <div class="pref-switch-with-arrow">
                  <label class="pref-switch" aria-label="Auto backup on stop">
                    <input type="checkbox" bind:checked={prefAutoBackupOnStop} />
                    <span class="switch-pill"></span>
                  </label>
                  <button
                    type="button"
                    class="pref-arrow-btn"
                    class:rotated={showBackupScopeOptions}
                    title="Toggle backup scope options"
                    aria-label="Toggle backup scope options"
                    on:click={() => (showBackupScopeOptions = !showBackupScopeOptions)}
                  >
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      <polyline points="6 9 12 15 18 9"/>
                    </svg>
                  </button>
                </div>
              </div>
              {#if showBackupScopeOptions}
                <div class="pref-sub-options-row">
                  <label class="pref-radio-pill" class:active={prefBackupScope === 'worlds'}>
                    <input type="radio" bind:group={prefBackupScope} value="worlds" />
                    <span>Worlds only</span>
                  </label>
                  <label class="pref-radio-pill" class:active={prefBackupScope === 'full'}>
                    <input type="radio" bind:group={prefBackupScope} value="full" />
                    <span>Entire instance</span>
                  </label>
                </div>
              {/if}

              <div class="pref-row">
                <div class="pref-row-info">
                  <span class="pref-row-title">Delete backups older than</span>
                </div>
                <div class="pref-select-wrapper">
                  <select bind:value={prefDeleteBackupsOlderThan} class="pref-select" aria-label="Delete backups older than">
                    <option value="7 days">7 days</option>
                    <option value="14 days">14 days</option>
                    <option value="30 days">30 days</option>
                    <option value="60 days">60 days</option>
                    <option value="90 days">90 days</option>
                    <option value="Never">Never</option>
                  </select>
                </div>
              </div>

              <div class="pref-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">Maximum backups per server</div>
                  <div class="pref-row-subtitle">Oldest are deleted first (0 keeps all)</div>
                </div>
                <div class="pref-stepper-control" role="group" aria-label="Maximum backups per server">
                  <span class="stepper-count">{prefMaxBackupsPerServer}</span>
                  <button
                    type="button"
                    class="stepper-btn"
                    aria-label="Decrease maximum backups"
                    on:click={() => (prefMaxBackupsPerServer = Math.max(0, prefMaxBackupsPerServer - 1))}
                  >
                    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true">
                      <line x1="5" x2="19" y1="12" y2="12"/>
                    </svg>
                  </button>
                  <button
                    type="button"
                    class="stepper-btn"
                    aria-label="Increase maximum backups"
                    on:click={() => (prefMaxBackupsPerServer = prefMaxBackupsPerServer + 1)}
                  >
                    <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" aria-hidden="true">
                      <line x1="12" x2="12" y1="5" y2="19"/>
                      <line x1="5" x2="19" y1="12" y2="12"/>
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </div>
        {:else}
          <!-- Remote Section -->
          <div class="pref-section">
            <h3 class="pref-section-heading">Remote Access (Iroh P2P)</h3>
            <div class="pref-card-group">
              <div class="pref-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">Iroh Relay Service</div>
                  <div class="pref-row-subtitle">Default encrypted community relay for peer hole-punching</div>
                </div>
                <span class="pref-badge-status">Connected</span>
              </div>
              <div class="pref-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">Direct Connection Fallback</div>
                  <div class="pref-row-subtitle">Attempt direct UDP hole-punching before relay forwarding</div>
                </div>
                <label class="pref-switch" aria-label="Direct Connection Fallback">
                  <input type="checkbox" checked disabled />
                  <span class="switch-pill"></span>
                </label>
              </div>
              <div class="pref-row">
                <div class="pref-row-info">
                  <div class="pref-row-title">OS Keychain Storage</div>
                  <div class="pref-row-subtitle">Remote credentials and auth secrets saved to device OS keychain only</div>
                </div>
                <span class="pref-badge-secure">Encrypted</span>
              </div>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- Keyboard Shortcuts Modal -->
{#if showShortcutsModal}
  <div class="modal-backdrop" role="presentation" on:click|self={() => (showShortcutsModal = false)}>
    <div class="shortcuts-dialog" role="dialog" aria-modal="true" aria-labelledby="shortcuts-dialog-title">
      <div class="dialog-header">
        <h2 id="shortcuts-dialog-title">Keyboard Shortcuts</h2>
        <button
          type="button"
          class="dialog-close-btn"
          aria-label="Close keyboard shortcuts"
          on:click={() => (showShortcutsModal = false)}
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
      <div class="shortcuts-list">
        <div class="shortcut-item">
          <span class="shortcut-desc">Open Preferences</span>
          <kbd class="shortcut-key">Ctrl + ,</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">Open Keyboard Shortcuts</span>
          <kbd class="shortcut-key">Ctrl + ?</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">Close Modal or Menu</span>
          <kbd class="shortcut-key">Escape</kbd>
        </div>
        <div class="shortcut-item">
          <span class="shortcut-desc">Toggle New Server Form</span>
          <kbd class="shortcut-key">Ctrl + N</kbd>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- About Redstone Modal -->
{#if showAboutModal}
  <div class="modal-backdrop" role="presentation" on:click|self={() => (showAboutModal = false)}>
    <div class="about-dialog" role="dialog" aria-modal="true" aria-labelledby="about-dialog-title">
      <div class="about-header">
        <div class="about-icon">R</div>
        <h2 id="about-dialog-title">Redstone</h2>
        <span class="about-version">Version 0.1.0</span>
        <button
          type="button"
          class="dialog-close-btn about-close-pos"
          aria-label="Close about dialog"
          on:click={() => (showAboutModal = false)}
        >
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
      <p class="about-description">
        Private, local-first Minecraft server host and manager. Runs entirely on your device with no mandatory account, no cloud dependency, and zero telemetry.
      </p>
      <div class="about-specs">
        <div class="about-spec-row">
          <span>Stack</span>
          <strong>Rust 2021 · Tauri 2 · Svelte · Iroh P2P</strong>
        </div>
        <div class="about-spec-row">
          <span>License</span>
          <strong>Open Source (MIT / Apache-2.0)</strong>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    min-height: 100vh;
    background: #0d0e12;
    color: #e5e7eb;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
  }

  :global(button) {
    font: inherit;
  }

  .app-shell {
    min-height: 100vh;
    background: #0d0e12;
    background-image: radial-gradient(circle at 50% 0%, rgba(27, 217, 106, 0.05) 0%, transparent 60%);
  }

  .topbar {
    height: 72px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4vw;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(18, 20, 26, 0.95);
    backdrop-filter: blur(12px);
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: 11px;
    color: #ffffff;
    font-size: 14px;
    font-weight: 800;
    letter-spacing: 0.04em;
    text-decoration: none;
  }

  .brand-mark {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    background: linear-gradient(135deg, #1bd96a 0%, #059669 100%);
    color: #0b120e;
    border-radius: 8px;
    font-size: 18px;
    font-weight: 800;
    box-shadow: 0 0 16px rgba(27, 217, 106, 0.35);
  }

  .device-status {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #9ca3af;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .device-status span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #10b981;
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.6);
  }

  .topbar-right {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .mode-switch-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.05);
    color: #e5e7eb;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .mode-switch-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.22);
    color: #ffffff;
  }

  .setup-view {
    width: min(1360px, calc(100% - 48px));
    margin: 0 auto;
    padding: 32px 0 64px;
  }

  .step-label {
    display: flex;
    align-items: center;
    gap: 12px;
    color: #9ca3af;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.08em;
  }

  .step-label span {
    color: #1bd96a;
    font-size: 14px;
    font-weight: 800;
  }

  .intro {
    max-width: 680px;
    padding: 16px 0 24px;
  }

  h1 {
    margin: 0 0 10px;
    font-size: 34px;
    font-weight: 800;
    letter-spacing: -0.02em;
    color: #ffffff;
    line-height: 1.15;
  }

  .intro p,
  .field-hint {
    margin: 0;
    color: #9ca3af;
    font-size: 14px;
    line-height: 1.6;
  }

  .settings-form {
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }

  .workspace-tabs {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 6px;
    padding: 5px;
    background: rgba(0, 0, 0, 0.45);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 11px;
    margin-bottom: 20px;
  }

  .workspace-tabs button {
    min-height: 38px;
    padding: 6px 10px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    color: #9ca3af;
    font-size: 12px;
    font-weight: 600;
    line-height: 1.2;
    cursor: pointer;
    transition: all 0.15s ease;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    white-space: nowrap;
  }

  .workspace-tabs button:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.06);
  }

  .workspace-tabs button.active {
    background: #1f2330;
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.15);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.4);
    font-weight: 700;
  }

  .tab-btn-icon {
    font-size: 13px;
    line-height: 1;
  }

  .update-count {
    min-width: 18px;
    height: 18px;
    display: inline-grid;
    place-items: center;
    margin-left: 6px;
    padding: 0 5px;
    border-radius: 9px;
    background: #f16436;
    color: #ffffff;
    font-size: 10px;
    font-weight: 800;
    vertical-align: middle;
    box-shadow: 0 0 8px rgba(241, 100, 54, 0.5);
  }

  .update-count.large {
    min-width: 22px;
    height: 22px;
    font-size: 11px;
  }

  .workspace-tabs button:focus-visible,
  .secondary-button:focus-visible {
    outline: 2px solid #1bd96a;
    outline-offset: -2px;
  }

  .tab-panel {
    min-width: 0;
    animation: panel-enter 160ms ease-out;
  }

  @keyframes panel-enter {
    from { opacity: 0.65; transform: translateY(3px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .panel-heading {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    flex-wrap: wrap;
    gap: 12px;
    padding: 4px 0 20px;
  }

  .panel-heading h2,
  .server-status h2 {
    margin: 0 0 6px;
    color: #ffffff;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .panel-heading p {
    margin: 0;
    color: #94a3b8;
    font-size: 13px;
    line-height: 1.5;
  }

  .server-settings-form {
    border-top: 0;
  }

  .secondary-button {
    min-height: 34px;
    padding: 0 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: #232736;
    color: #e2e8f0;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    transition: all 0.15s ease;
  }

  .secondary-button:hover:not(:disabled) {
    background: #2d3245;
    border-color: rgba(34, 197, 94, 0.4);
    color: #ffffff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
  }

  .secondary-button:active:not(:disabled) {
    transform: scale(0.98);
  }

  .secondary-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .server-status {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 18px;
    padding: 22px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .world-properties {
    display: grid;
    gap: 12px;
  }

  .world-properties-gui {
    display: flex;
    flex-direction: column;
    gap: 20px;
    margin: 8px 0 16px;
  }

  .props-section {
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .props-section h4 {
    margin: 0;
    font-size: 11px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: #94a3b8;
  }

  .props-grid-3 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
  }

  .props-grid-4 {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 12px;
    align-items: flex-end;
  }

  .prop-toggle-item {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .toggle-label {
    font-size: 11px;
    font-weight: 700;
    color: #53625b;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .prop-toggle-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 38px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: #181b25;
    color: #cbd5e1;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .prop-toggle-btn:hover {
    background: #1f2333;
    border-color: rgba(255, 255, 255, 0.16);
    color: #ffffff;
  }

  .prop-toggle-btn.active {
    background: rgba(34, 197, 94, 0.1);
    border-color: #22c55e;
    color: #4ade80;
  }

  .toggle-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #4b5563;
    transition: background 0.15s ease;
  }

  .prop-toggle-btn.active .toggle-dot {
    background: #22c55e;
    box-shadow: 0 0 6px rgba(34, 197, 94, 0.6);
  }

  .props-toggles-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: 10px;
  }

  .prop-toggle-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    padding: 10px 12px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }

  .prop-toggle-card:hover {
    background: #1f2333;
    border-color: rgba(255, 255, 255, 0.16);
  }

  .prop-toggle-card.checked {
    background: rgba(34, 197, 94, 0.1);
    border-color: #22c55e;
  }

  .toggle-card-title {
    font-size: 12px;
    font-weight: 700;
    color: #f3f4f6;
  }

  .toggle-card-status {
    font-size: 11px;
    color: #94a3b8;
    margin-top: 2px;
  }

  .prop-toggle-card.checked .toggle-card-status {
    color: #4ade80;
    font-weight: 700;
  }

  .props-sliders-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 16px;
  }

  .slider-field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .slider-field input[type="range"] {
    accent-color: #22c55e;
    cursor: pointer;
  }

  .slider-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .slider-header span {
    font-size: 11px;
    color: #94a3b8;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .slider-header strong {
    font-size: 13px;
    color: #f3f4f6;
  }

  .advanced-props-details {
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 14px 18px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .advanced-props-details summary {
    font-size: 13px;
    font-weight: 700;
    color: #f3f4f6;
    cursor: pointer;
    user-select: none;
    outline: none;
  }

  .advanced-props-body {
    margin-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .custom-props-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 220px;
    overflow-y: auto;
  }

  .custom-prop-row {
    display: grid;
    grid-template-columns: 140px 1fr 32px;
    gap: 8px;
    align-items: center;
  }

  .custom-prop-key {
    font-size: 12px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #94a3b8;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .custom-prop-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: #11131a;
    color: #f3f4f6;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
  }

  .custom-prop-input:focus {
    border-color: #22c55e;
    outline: none;
  }

  .remove-prop-btn {
    padding: 4px 0;
    text-align: center;
    font-size: 16px;
    line-height: 1;
    height: 30px;
  }

  .add-prop-row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 8px;
    margin-top: 8px;
  }

  .add-prop-row input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: #11131a;
    color: #f3f4f6;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
  }

  .add-prop-row input:focus {
    border-color: #22c55e;
    outline: none;
  }

  .world-properties-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .jvm-settings {
    display: grid;
    gap: 12px;
  }

  .jvm-presets {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .jvm-presets button {
    min-height: 38px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    background: #181b25;
    color: #cbd5e1;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .jvm-presets button:hover {
    background: #1f2333;
    border-color: rgba(255, 255, 255, 0.16);
    color: #ffffff;
  }

  .jvm-presets button.chosen {
    border-color: #22c55e;
    box-shadow: inset 0 0 0 1px #22c55e;
    background: rgba(34, 197, 94, 0.1);
    color: #4ade80;
  }

  .server-status h2 {
    font-family: inherit;
    font-size: 16px;
    font-weight: 700;
  }

  .server-status p {
    max-width: 460px;
    margin: 0;
    color: #94a3b8;
    font-size: 13px;
    line-height: 1.55;
  }

  .status-kicker {
    display: block;
    margin-bottom: 4px;
    color: #10b981;
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .status-pill {
    flex: 0 0 auto;
    padding: 5px 10px;
    border-radius: 20px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background: rgba(255, 255, 255, 0.05);
    color: #cbd5e1;
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .status-pill.running {
    border-color: rgba(34, 197, 94, 0.35);
    background: rgba(34, 197, 94, 0.12);
    color: #4ade80;
  }

  .status-pill.crashed {
    border-color: rgba(239, 68, 68, 0.35);
    background: rgba(239, 68, 68, 0.12);
    color: #f87171;
  }

  .server-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 10px;
    padding-top: 18px;
  }

  fieldset {
    min-width: 0;
    margin: 0;
    padding: 24px 0 28px;
    border: 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  legend {
    padding: 0;
    color: #f3f4f6;
    font-size: 16px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .field-hint {
    margin-top: 6px;
    font-size: 13px;
    color: #94a3b8;
    line-height: 1.5;
  }

  .join-card {
    display: grid;
    gap: 16px;
    padding: 22px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    background: #181b25;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.3);
    margin-top: 20px;
  }

  .join-card-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .join-card-header h3 {
    margin: 4px 0 0;
    color: #ffffff;
    font-family: inherit;
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .join-address-box {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 18px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: #10121a;
    border-radius: 10px;
  }

  .join-address-text {
    font-size: 16px;
    color: #22c55e;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    overflow-wrap: anywhere;
    letter-spacing: 0.02em;
  }

  .join-host {
    font-weight: 700;
  }

  .join-port {
    color: #94a3b8;
  }

  .join-loading {
    color: #94a3b8;
    font-size: 14px;
    font-style: normal;
  }

  .join-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .join-actions button {
    min-height: 36px;
    min-width: 125px;
    padding: 0 16px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    font-size: 12px;
    font-weight: 600;
  }

  .qr-modal-panel {
    display: grid;
    gap: 12px;
    padding: 18px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    justify-items: center;
    text-align: center;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  }

  .qr-display {
    display: inline-block;
    padding: 12px;
    background: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
  }

  .qr-payload-text code {
    font-size: 12px;
    color: #22c55e;
    background: #10121a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 4px 8px;
    border-radius: 4px;
  }

  .secret-field-group {
    display: grid;
    gap: 12px;
  }

  .password-input-row {
    display: flex;
    gap: 8px;
  }

  .password-input-row input {
    flex: 1;
    min-width: 0;
  }

  .dns-sync-actions {
    display: flex;
    gap: 10px;
    margin-top: 4px;
  }

  .dns-fields {
    display: grid;
    gap: 16px;
  }

  .dns-fields .field-hint {
    margin-top: -10px;
  }

  .address-fields {
    display: grid;
    grid-template-columns: minmax(100px, 0.8fr) 12px minmax(0, 1.4fr);
    align-items: end;
    gap: 10px;
  }

  .hostname-dot {
    padding-bottom: 10px;
    color: #53625b;
    text-align: center;
  }

  .form-field {
    display: grid;
    gap: 7px;
    color: #94a3b8;
    font-size: 12px;
    font-weight: 600;
  }

  .form-field input,
  .form-field select {
    width: 100%;
    min-height: 42px;
    padding: 0 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: #11131a;
    color: #f3f4f6;
    font: inherit;
    font-size: 13px;
    transition: all 0.15s ease;
  }

  .form-field input:focus,
  .form-field select:focus {
    border-color: #22c55e;
    outline: none;
    box-shadow: 0 0 0 2px rgba(34, 197, 94, 0.2);
  }

  .preview-note {
    margin: 4px 0 0;
    color: #f59e0b;
    font-size: 12px;
    line-height: 1.5;
  }

  .mod-catalog {
    display: grid;
    gap: 16px;
  }

  .installed-mods {
    display: grid;
    gap: 12px;
  }

  .updates-panel {
    display: grid;
    gap: 12px;
    padding: 22px 0;
    border-bottom: 1px solid rgba(22, 35, 31, 0.18);
  }

  .updates-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .updates-heading h3 {
    margin: 0;
    color: #ffffff;
    font-size: 13px;
  }

  .updates-list {
    margin: 0;
    padding: 0;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    list-style: none;
  }

  .updates-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 11px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .updates-list li > div {
    min-width: 0;
    display: grid;
    gap: 4px;
  }

  .updates-list strong {
    overflow-wrap: anywhere;
    color: #f3f4f6;
    font-size: 12px;
  }

  .updates-list span {
    color: #94a3b8;
    font-size: 10px;
  }

  .update-mod-button {
    min-width: 68px;
    min-height: 34px;
    flex: 0 0 auto;
    padding: 0 10px;
    border: 1px solid #22c55e;
    border-radius: 6px;
    background: rgba(34, 197, 94, 0.1);
    color: #4ade80;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }

  .update-mod-button:hover:not(:disabled) {
    background: rgba(34, 197, 94, 0.2);
  }

  .update-mod-button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .installed-mods-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .refresh-mods-button {
    width: 34px;
    height: 34px;
    flex: 0 0 34px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: #232736;
    color: #f3f4f6;
    font-size: 16px;
    cursor: pointer;
  }

  .refresh-mods-button:disabled,
  .installed-action:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .installed-mod-list {
    margin: 0;
    padding: 0;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    list-style: none;
  }

  .installed-mod-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .installed-mod-name {
    min-width: 0;
    display: grid;
    gap: 4px;
  }

  .installed-mod-name strong {
    overflow-wrap: anywhere;
    color: #f3f4f6;
    font-size: 12px;
  }

  .installed-mod-name span {
    color: #94a3b8;
    font-size: 10px;
  }

  .installed-action {
    min-width: 62px;
    min-height: 32px;
    padding: 0 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: #232736;
    color: #e2e8f0;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .installed-action:hover:not(:disabled) {
    background: #2d3245;
    color: #ffffff;
  }

  .installed-action.remove-action {
    color: #f87171;
    border-color: rgba(239, 68, 68, 0.35);
  }

  .catalog-message {
    margin: 8px 0;
    color: #94a3b8;
    font-size: 13px;
    line-height: 1.5;
  }

  .catalog-message.error {
    color: #f87171;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.25);
    border-radius: 8px;
    padding: 8px 12px;
  }

  /* Authentically themed mod catalog */
  .mod-catalog {
    display: grid;
    gap: 16px;
    background: #151720;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 20px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.25);
  }

  /* Catalog Source Selector: Compact 50/50 Buttons with High-Contrast SVGs */
  .catalog-source-selector {
    display: inline-grid;
    grid-template-columns: 1fr 1fr;
    align-items: center;
    gap: 3px;
    margin-bottom: 0;
    background: rgba(0, 0, 0, 0.45);
    padding: 3px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    width: auto;
    min-width: 228px;
    max-width: 240px;
    flex-shrink: 0;
    box-sizing: border-box;
  }

  .catalog-source-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 5px 12px;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: #9ca3af;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    min-height: 32px;
    box-sizing: border-box;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .catalog-source-btn .official-logo {
    display: inline-block;
    flex-shrink: 0;
    transition: color 0.15s ease;
  }

  .catalog-source-btn .official-logo.modrinth-logo {
    color: #1bd96a;
  }

  .catalog-source-btn .official-logo.curseforge-logo {
    color: #f16436;
  }

  .catalog-source-btn:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.08);
  }

  .catalog-source-btn.chosen {
    background: #14866a;
    color: #ffffff;
    font-weight: 800;
    box-shadow: 0 0 16px rgba(20, 134, 106, 0.35);
  }

  .catalog-source-btn.source-modrinth.chosen {
    background: #1bd96a;
    color: #0b120e;
    box-shadow: 0 0 16px rgba(27, 217, 106, 0.4);
  }

  .catalog-source-btn.source-modrinth.chosen .official-logo.modrinth-logo {
    color: #0b120e;
  }

  .catalog-source-btn.source-curseforge.chosen {
    background: #f16436;
    color: #ffffff;
    box-shadow: 0 0 16px rgba(241, 100, 54, 0.4);
  }

  .catalog-source-btn.source-curseforge.chosen .official-logo.curseforge-logo {
    color: #ffffff;
  }

  /* Functioning Category Chip Bar */
  .category-chip-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 0 10px;
  }

  .category-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #d1d5db;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    user-select: none;
  }

  .category-chip:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-1px);
  }

  .theme-modrinth .category-chip.active {
    background: rgba(27, 217, 106, 0.18);
    color: #1bd96a;
    border-color: #1bd96a;
    box-shadow: 0 0 10px rgba(27, 217, 106, 0.25);
  }

  .theme-curseforge .category-chip.active {
    background: rgba(241, 100, 54, 0.18);
    color: #ff7324;
    border-color: #f16436;
    box-shadow: 0 0 10px rgba(241, 100, 54, 0.25);
  }

  /* CurseForge Key Section */
  .curseforge-key-section {
    margin-bottom: 12px;
    padding: 16px;
    background: #1c1d24;
    border: 1px solid rgba(241, 100, 54, 0.3);
    border-radius: 10px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .password-input-row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .password-input-row input {
    flex: 1;
    background: #13141a;
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: #ffffff;
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 13px;
  }

  .password-input-row input:focus {
    border-color: #f16436;
    outline: none;
    box-shadow: 0 0 0 2px rgba(241, 100, 54, 0.2);
  }

  /* CurseForge Key Hint Callout & Status Badges */
  .cf-key-hint-box {
    margin-top: 14px;
    padding: 12px 14px;
    background: rgba(241, 100, 54, 0.08);
    border: 1px solid rgba(241, 100, 54, 0.25);
    border-radius: 9px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }

  .cf-key-hint-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #ff7324;
    font-size: 13px;
    font-weight: 700;
  }

  .cf-key-hint-header .hint-svg {
    color: #f16436;
    flex-shrink: 0;
  }

  .cf-key-hint-desc {
    margin: 0;
    color: #d1d5db;
    line-height: 1.5;
    font-size: 12px;
  }

  .cf-key-hint-steps {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: #9ca3af;
    font-size: 12px;
  }

  .cf-key-hint-steps .steps-title {
    font-weight: 700;
    color: #e5e7eb;
  }

  .cf-key-hint-steps ol {
    margin: 2px 0 0;
    padding-left: 20px;
    color: #d1d5db;
    line-height: 1.5;
  }

  .cf-key-hint-steps li {
    margin-bottom: 2px;
  }

  .cf-key-hint-steps a {
    color: #ff7324;
    font-weight: 700;
    text-decoration: underline;
  }

  .cf-key-hint-steps a:hover {
    color: #ff8c4a;
  }

  .cf-key-security-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    color: #9ca3af;
    font-size: 11px;
    line-height: 1.4;
  }

  .cf-key-security-badge svg {
    color: #22c55e;
    flex-shrink: 0;
  }

  .cf-key-active-badge {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: #22c55e;
    font-size: 12px;
    font-weight: 600;
  }

  .cf-key-active-badge svg {
    flex-shrink: 0;
  }

  .curseforge-locked-banner {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 20px;
    background: rgba(241, 100, 54, 0.08);
    border: 1px dashed rgba(241, 100, 54, 0.35);
    border-radius: 12px;
    margin: 16px 0;
    color: #d1d5db;
  }

  .locked-banner-icon {
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    background: rgba(241, 100, 54, 0.18);
    border: 1px solid rgba(241, 100, 54, 0.35);
    border-radius: 10px;
    color: #ff7324;
    flex-shrink: 0;
  }

  .locked-banner-content h4 {
    margin: 0 0 4px;
    color: #ffffff;
    font-size: 15px;
    font-weight: 700;
  }

  .locked-banner-content p {
    margin: 0;
    font-size: 13px;
    color: #9ca3af;
    line-height: 1.45;
  }

  .project-type-pill.key-locked,
  .env-toggle-btn.key-locked,
  .catalog-search-field input:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .catalog-search-field input:disabled {
    background: rgba(0, 0, 0, 0.25);
    color: #6b7280;
  }

  /* Mod Results List */
  .mod-results {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 12px;
  }

  .mod-result {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding: 14px 16px;
    background: #1a1c26;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 12px;
    transition: all 0.18s ease;
    min-width: 0;
    box-sizing: border-box;
  }

  .mod-result:hover {
    background: #202330;
    border-color: rgba(255, 255, 255, 0.14);
    transform: translateY(-1px);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.3);
  }

  .theme-curseforge .mod-result {
    background: #19191d;
    border-color: rgba(255, 255, 255, 0.06);
  }

  .theme-curseforge .mod-result:hover {
    background: #212127;
    border-color: rgba(241, 100, 54, 0.35);
  }

  .mod-thumbnail,
  .mod-mark {
    width: 52px;
    height: 52px;
    border-radius: 12px;
    object-fit: cover;
    flex-shrink: 0;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.35);
  }

  .theme-modrinth .mod-mark {
    display: grid;
    place-items: center;
    background: linear-gradient(135deg, #1bd96a 0%, #0d7942 100%);
    color: #ffffff;
    font-size: 20px;
    font-weight: 800;
  }

  .theme-curseforge .mod-mark {
    display: grid;
    place-items: center;
    background: linear-gradient(135deg, #ff7324 0%, #b8360d 100%);
    color: #ffffff;
    font-size: 20px;
    font-weight: 800;
  }

  .mod-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow: hidden;
  }

  .mod-title-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    min-width: 0;
  }

  .mod-title-text {
    font-size: 15px;
    font-weight: 700;
    color: #f3f4f6;
    transition: color 0.15s ease;
    overflow-wrap: anywhere;
    word-break: break-word;
  }

  .theme-modrinth .mod-result:hover .mod-title-text {
    color: #1bd96a;
  }

  .theme-curseforge .mod-result:hover .mod-title-text {
    color: #ff7324;
  }

  .mod-source-badge {
    display: inline-block;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    background: rgba(255, 255, 255, 0.08);
    color: #9ca3af;
  }

  .mod-env-badge {
    display: inline-block;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    background: rgba(56, 189, 248, 0.15);
    color: #38bdf8;
    border: 1px solid rgba(56, 189, 248, 0.25);
  }

  .mod-meta-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: #9ca3af;
  }

  .mod-author-tag {
    color: #9ca3af;
  }

  .mod-downloads-count {
    color: #6b7280;
    font-weight: 600;
  }

  .mod-desc-text {
    margin: 2px 0 0;
    color: #9ca3af;
    font-size: 12px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .mod-categories-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }

  .mod-category-pill {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: #9ca3af;
    border: 1px solid rgba(255, 255, 255, 0.07);
  }

  /* Mod Install Action Button */
  .install-mod-button {
    min-width: 90px;
    min-height: 36px;
    padding: 0 16px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.18s ease;
    border: none;
  }

  .theme-modrinth .install-mod-button {
    background: #1bd96a;
    color: #0b120e;
    box-shadow: 0 2px 10px rgba(27, 217, 106, 0.25);
  }

  .theme-modrinth .install-mod-button:hover:not(:disabled) {
    background: #25ea77;
    box-shadow: 0 4px 16px rgba(27, 217, 106, 0.45);
    transform: translateY(-1px);
  }

  .theme-curseforge .install-mod-button {
    background: #f16436;
    color: #ffffff;
    box-shadow: 0 2px 10px rgba(241, 100, 54, 0.25);
  }

  .theme-curseforge .install-mod-button:hover:not(:disabled) {
    background: #ff7324;
    box-shadow: 0 4px 16px rgba(241, 100, 54, 0.45);
    transform: translateY(-1px);
  }

  .install-mod-button:disabled {
    opacity: 0.6;
    cursor: default;
    background: #282a35;
    color: #9ca3af;
    box-shadow: none;
  }

  .external-link-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 14px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: #d1d5db;
    text-decoration: none;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    transition: all 0.15s ease;
  }

  .external-link-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.2);
  }

  /* ==========================================================================
     Hamburger Dropdown & Theme Selector (Image 1)
     ========================================================================== */
  .options-menu-container {
    position: relative;
  }

  .options-dropdown-menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    min-width: 240px;
    background: #242630;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 14px;
    padding: 12px;
    box-shadow: 0 16px 36px rgba(0, 0, 0, 0.55);
    z-index: 100;
    backdrop-filter: blur(12px);
    display: flex;
    flex-direction: column;
    gap: 4px;
    animation: menu-pop 0.15s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes menu-pop {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .theme-switch-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    align-items: center;
    background: rgba(0, 0, 0, 0.45);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    padding: 3px;
    gap: 4px;
    margin: 4px 6px 8px;
  }

  .theme-switch-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    padding: 7px 12px;
    border-radius: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: #94a3b8;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    user-select: none;
    min-height: 34px;
  }

  .theme-switch-btn svg {
    flex-shrink: 0;
    color: #94a3b8;
    transition: all 0.2s ease;
  }

  .theme-switch-btn:hover {
    color: #f8fafc;
    background: rgba(255, 255, 255, 0.08);
  }

  .theme-switch-btn:hover svg {
    transform: rotate(12deg) scale(1.1);
  }

  .theme-switch-btn.theme-light.active {
    background: linear-gradient(135deg, #ffffff, #f8fafc);
    color: #0f172a;
    border-color: #cbd5e1;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 12px rgba(245, 158, 11, 0.2);
    font-weight: 700;
  }

  .theme-switch-btn.theme-light.active svg {
    color: #d97706;
    transform: scale(1.1);
  }

  .theme-switch-btn.theme-dark.active {
    background: linear-gradient(135deg, #1e293b, #0f172a);
    color: #f8fafc;
    border-color: rgba(56, 189, 248, 0.35);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.4), 0 0 12px rgba(56, 189, 248, 0.25);
    font-weight: 700;
  }

  .theme-switch-btn.theme-dark.active svg {
    color: #38bdf8;
    transform: scale(1.1);
  }

  .dropdown-divider {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 4px 0 6px;
  }

  .dropdown-menu-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    color: #e5e7eb;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    transition: background 0.12s ease;
  }

  .dropdown-menu-item:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #ffffff;
  }

  .item-shortcut {
    font-family: inherit;
    font-size: 11px;
    color: #9ca3af;
    background: rgba(255, 255, 255, 0.05);
    padding: 2px 5px;
    border-radius: 4px;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  /* ==========================================================================
     Preferences Modal (Image 2) & Dialogs
     ========================================================================== */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    backdrop-filter: blur(6px);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    animation: fade-in 0.15s ease-out;
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .preferences-dialog {
    width: 100%;
    max-width: 600px;
    max-height: 85vh;
    background: #1e2028;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 16px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.65);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: modal-enter 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes modal-enter {
    from {
      opacity: 0;
      transform: scale(0.96) translateY(8px);
    }
    to {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  .pref-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    background: #191b22;
  }

  .pref-tool-btn,
  .pref-close-btn {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #e5e7eb;
    display: grid;
    place-items: center;
    cursor: pointer;
    font-size: 13px;
    transition: all 0.15s ease;
  }

  .pref-tool-btn:hover,
  .pref-close-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
  }

  .pref-segmented-nav {
    display: inline-flex;
    padding: 3px;
    background: #14151b;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 9px;
    gap: 3px;
  }

  .pref-segment-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    border-radius: 7px;
    background: transparent;
    border: none;
    color: #9ca3af;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pref-segment-btn.active {
    background: #2b2e3a;
    color: #ffffff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
  }

  .pref-search-bar {
    padding: 8px 18px;
    background: #15171f;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .pref-search-bar input {
    width: 100%;
    padding: 8px 12px;
    background: #1b1d26;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: #ffffff;
    font-size: 13px;
  }

  .pref-body-scroll {
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .pref-section-heading {
    margin: 0 0 10px 4px;
    font-size: 13px;
    font-weight: 700;
    color: #f3f4f6;
    letter-spacing: 0.02em;
  }

  .pref-card-group {
    background: #232631;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    overflow: hidden;
  }

  .pref-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 13px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    gap: 12px;
  }

  .pref-row:last-child {
    border-bottom: none;
  }

  .pref-row-info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pref-row-title {
    font-size: 13px;
    font-weight: 600;
    color: #f3f4f6;
  }

  .pref-row-subtitle {
    font-size: 11px;
    color: #8b92a2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 360px;
  }

  .pref-icon-action-btn {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: #e5e7eb;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pref-icon-action-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
  }

  /* Switch Toggle */
  .pref-switch {
    position: relative;
    display: inline-block;
    width: 44px;
    height: 24px;
    flex-shrink: 0;
    cursor: pointer;
  }

  .pref-switch input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .switch-pill {
    position: absolute;
    inset: 0;
    background-color: #373a46;
    border-radius: 24px;
    transition: 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .switch-pill:before {
    position: absolute;
    content: "";
    height: 18px;
    width: 18px;
    left: 3px;
    bottom: 3px;
    background-color: #ffffff;
    border-radius: 50%;
    transition: 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }

  .pref-switch input:checked + .switch-pill {
    background-color: #3b82f6;
  }

  .pref-switch input:checked + .switch-pill:before {
    transform: translateX(20px);
  }

  .pref-switch-with-arrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .pref-arrow-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.05);
    color: #94a3b8;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pref-arrow-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
  }

  .pref-arrow-btn.rotated svg {
    transform: rotate(180deg);
  }

  .pref-arrow-btn svg {
    transition: transform 0.2s ease;
  }

  .pref-sub-options-row {
    display: flex;
    gap: 8px;
    padding: 10px 14px;
    background: rgba(0, 0, 0, 0.25);
    border-radius: 8px;
    margin: 4px 0 10px;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  .pref-radio-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: #94a3b8;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .pref-radio-pill.active {
    background: #1e293b;
    color: #ffffff;
    border: 1px solid rgba(56, 189, 248, 0.4);
  }

  .pref-radio-pill input {
    margin: 0;
  }

  /* Custom Pref Select */
  .pref-select-wrapper {
    position: relative;
  }

  .pref-select {
    appearance: none;
    background: #1a1c24;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: #e5e7eb;
    padding: 6px 28px 6px 12px;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%239ca3af' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
    background-repeat: no-repeat;
    background-position: right 8px center;
    background-size: 14px;
  }

  /* Stepper */
  .pref-stepper-control {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: #1a1c24;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 2px 4px;
  }

  .stepper-count {
    min-width: 28px;
    text-align: center;
    font-size: 13px;
    font-weight: 700;
    color: #f3f4f6;
  }

  .stepper-btn {
    width: 26px;
    height: 26px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    color: #ffffff;
    font-size: 14px;
    font-weight: 700;
    cursor: pointer;
    display: grid;
    place-items: center;
    transition: background 0.15s ease;
  }

  .stepper-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  /* Shortcuts Dialog */
  .shortcuts-dialog,
  .about-dialog {
    width: 100%;
    max-width: 480px;
    background: #1e2028;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 16px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.65);
    padding: 22px;
    animation: modal-enter 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .dialog-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .dialog-header h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
  }

  .dialog-close-btn {
    background: transparent;
    border: none;
    color: #9ca3af;
    font-size: 16px;
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }

  .dialog-close-btn:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.08);
  }

  .shortcuts-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .shortcut-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    background: #252834;
    border-radius: 8px;
  }

  .shortcut-desc {
    font-size: 13px;
    color: #e5e7eb;
  }

  .shortcut-key {
    background: #181921;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 3px 8px;
    font-size: 11px;
    font-weight: 700;
    color: #60a5fa;
  }

  .about-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    position: relative;
    padding-bottom: 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .about-icon {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: linear-gradient(135deg, #1bd96a 0%, #059669 100%);
    color: #0b120e;
    font-size: 26px;
    font-weight: 900;
    display: grid;
    place-items: center;
    box-shadow: 0 0 20px rgba(27, 217, 106, 0.4);
    margin-bottom: 8px;
  }

  .about-version {
    font-size: 12px;
    color: #9ca3af;
  }

  .about-close-pos {
    position: absolute;
    top: 0;
    right: 0;
  }

  .about-description {
    font-size: 13px;
    line-height: 1.5;
    color: #d1d5db;
    margin: 14px 0;
    text-align: center;
  }

  .about-specs {
    background: #252834;
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .about-spec-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
  }

  .about-spec-row span {
    color: #9ca3af;
  }

  .about-spec-row strong {
    color: #ffffff;
  }

  .pref-badge-status {
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 700;
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;
    border: 1px solid rgba(34, 197, 94, 0.3);
  }

  .pref-badge-secure {
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 700;
    background: rgba(59, 130, 246, 0.15);
    color: #60a5fa;
    border: 1px solid rgba(59, 130, 246, 0.3);
  }

  /* ==========================================================================
     Authentic Category Drawer & Spacious Catalog Layout (Image 3)
     ========================================================================== */
  .catalog-expanded-panel {
    max-width: 100%;
  }

  .catalog-title-group h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 800;
    color: #ffffff;
  }

  .catalog-title-group p {
    margin: 4px 0 0;
    font-size: 13px;
    color: #9ca3af;
  }

  .catalog-active-meta {
    margin-top: 8px;
  }

  .active-loader-chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 12px;
    color: #d1d5db;
  }

  .loader-plugin-tag {
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(34, 197, 94, 0.15);
    color: #22c55e;
    font-size: 10px;
    font-weight: 700;
  }

  /* Two Column Workbench Layout */
  .catalog-workbench-layout {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    width: 100%;
    min-width: 0;
  }

  /* Left: Collapsible Authentic Category Sidebar (Image 3) */
  .catalog-category-sidebar {
    width: 220px;
    flex-shrink: 0;
    background: #1a1c26;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 10px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    transition: width 0.2s ease;
    box-sizing: border-box;
  }

  .catalog-category-sidebar.collapsed {
    width: 160px;
  }

  .catalog-category-sidebar.key-locked {
    opacity: 0.6;
    transition: opacity 0.2s ease;
  }

  .catalog-category-sidebar.key-locked .category-drawer-item {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.85);
    pointer-events: none;
  }

  .category-header-title-wrap {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .key-locked-badge {
    font-size: 10px;
    font-weight: 700;
    background: rgba(241, 100, 54, 0.18);
    color: #ff7324;
    border: 1px solid rgba(241, 100, 54, 0.35);
    padding: 1px 6px;
    border-radius: 4px;
    letter-spacing: 0.02em;
  }

  .category-sidebar-header-btn {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 10px;
    border-radius: 8px;
    background: transparent;
    border: none;
    color: #f3f4f6;
    font-size: 15px;
    font-weight: 700;
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .category-sidebar-header-btn:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .category-collapse-arrow {
    font-size: 14px;
    color: #9ca3af;
  }

  .category-drawer-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin-top: 8px;
    max-height: 520px;
    overflow-y: auto;
    padding-right: 6px;
    scrollbar-width: thin;
    scrollbar-color: rgba(255, 255, 255, 0.28) transparent;
  }

  .category-drawer-list::-webkit-scrollbar {
    width: 3px;
  }

  .category-drawer-list::-webkit-scrollbar-track {
    background: transparent;
  }

  .category-drawer-list::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.28);
    border-radius: 0;
  }

  .category-drawer-list::-webkit-scrollbar-thumb:hover {
    background: #22c55e;
  }

  .category-drawer-list::-webkit-scrollbar-button {
    display: none;
    width: 0;
    height: 0;
  }

  .category-drawer-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 7px 10px;
    border-radius: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: #cbd5e1;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
    user-select: none;
  }

  .category-drawer-item .chip-icon {
    width: 20px;
    height: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-right: 8px;
  }

  .category-drawer-item .chip-icon svg {
    width: 15px;
    height: 15px;
    flex-shrink: 0;
  }

  .category-drawer-item .chip-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.2;
  }

  .chip-active-check {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: 6px;
    color: #22c55e;
    flex-shrink: 0;
  }

  .category-drawer-item:hover {
    background: rgba(255, 255, 255, 0.06);
    color: #ffffff;
  }

  .theme-modrinth .category-drawer-item.active {
    background: rgba(27, 217, 106, 0.15);
    color: #1bd96a;
    border-color: rgba(27, 217, 106, 0.35);
    font-weight: 700;
  }

  .theme-curseforge .category-drawer-item.active {
    background: rgba(241, 100, 54, 0.15);
    color: #ff7324;
    border-color: rgba(241, 100, 54, 0.35);
    font-weight: 700;
  }

  /* Right: Main Catalog Area */
  .catalog-body-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
    container-type: inline-size;
    container-name: catalogArea;
  }

  .catalog-controls-deck {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 8px 12px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
  }

  .catalog-controls-top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 8px;
    width: 100%;
    min-width: 0;
  }

  /* Official SVGs */
  .official-logo {
    display: inline-block;
    vertical-align: middle;
    flex-shrink: 0;
  }

  /* Project Type Pills: Compact, Never Bloated */
  .project-type-selector {
    display: inline-flex;
    align-items: center;
    background: rgba(0, 0, 0, 0.35);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    padding: 3px;
    gap: 2px;
    max-width: 100%;
    overflow-x: auto;
    flex-shrink: 0;
    box-sizing: border-box;
    scrollbar-width: none;
  }

  .project-type-selector::-webkit-scrollbar {
    display: none;
    width: 0;
    height: 0;
  }

  .project-type-pill {
    padding: 5px 11px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid transparent;
    color: #9ca3af;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-height: 32px;
    box-sizing: border-box;
    white-space: nowrap;
    flex: 0 0 auto;
  }

  .project-type-pill:hover:not(:disabled) {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.06);
  }

  .project-type-pill.active {
    background: #282b37;
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.14);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
  }

  .project-type-pill.locked,
  .project-type-pill:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .locked-loader-badge {
    font-size: 9px;
    background: rgba(239, 68, 68, 0.15);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
    padding: 1px 5px;
    border-radius: 4px;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  /* Environment Toggle (Centered, High-contrast, Compact) */
  .env-toggle-container {
    display: flex;
    justify-content: center;
    align-items: center;
    width: 100%;
    padding-top: 4px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    margin: 0;
    box-sizing: border-box;
  }

  .env-toggle-group {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    background: rgba(15, 23, 42, 0.65);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 7px;
    padding: 2px;
    box-sizing: border-box;
  }

  .env-toggle-btn {
    padding: 3px 12px;
    border-radius: 5px;
    background: transparent;
    border: 1px solid transparent;
    color: #94a3b8;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
    min-height: 24px;
  }

  .env-toggle-btn:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.08);
  }

  .env-toggle-btn.active {
    background: rgba(255, 255, 255, 0.16);
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.22);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }

  .theme-modrinth .env-toggle-btn.active {
    background: rgba(27, 217, 106, 0.22);
    color: #1bd96a;
    border-color: rgba(27, 217, 106, 0.4);
  }

  .theme-curseforge .env-toggle-btn.active {
    background: rgba(241, 100, 54, 0.22);
    color: #ff7324;
    border-color: rgba(241, 100, 54, 0.4);
  }

  /* Catalog Responsive Compact Adaptations */
  @container catalogArea (max-width: 580px) {
    .catalog-controls-top-row {
      gap: 6px;
    }

    .mod-result {
      grid-template-columns: 42px minmax(0, 1fr);
      grid-template-rows: auto auto;
      align-items: flex-start;
      gap: 10px;
      padding: 10px 12px;
    }

    .mod-thumbnail,
    .mod-mark {
      width: 42px;
      height: 42px;
    }

    .install-mod-button,
    .external-link-btn {
      grid-column: 1 / -1;
      width: 100%;
      min-height: 32px;
      margin-top: 4px;
      justify-content: center;
      text-align: center;
    }
  }

  @container catalogArea (max-width: 440px) {
    .env-toggle-btn {
      padding: 3px 8px;
      font-size: 10px;
    }

    .project-type-pill {
      font-size: 10px;
      padding: 4px 8px;
    }

    .mod-title-text {
      font-size: 13px;
    }

    .mod-desc-text {
      font-size: 11px;
    }
  }

  /* Catalog Search Input */
  .catalog-search-bar-row {
    width: 100%;
  }

  .catalog-search-field {
    width: 100%;
  }

  .catalog-input-container {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
  }

  .search-glass-icon {
    position: absolute;
    left: 14px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    color: #9ca3af;
  }

  .catalog-input-container input {
    width: 100%;
    padding: 10px 40px 10px 38px;
    background: #171822;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 10px;
    color: #ffffff;
    font-size: 13px;
    transition: all 0.15s ease;
  }

  .catalog-input-container input:focus {
    border-color: #3b82f6;
    outline: none;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.2);
  }

  .search-clear-action-btn {
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: rgba(255, 255, 255, 0.08);
    border: none;
    color: #9ca3af;
    cursor: pointer;
    padding: 3px;
    border-radius: 50%;
    width: 22px;
    height: 22px;
    transition: all 0.15s ease;
  }

  .search-clear-action-btn:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.18);
  }

  .mod-type-pill {
    display: inline-block;
    padding: 2px 7px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    background: rgba(168, 85, 247, 0.15);
    color: #c084fc;
    border: 1px solid rgba(168, 85, 247, 0.3);
  }

  .mod-mark {
    display: grid;
    place-items: center;
    background: #242836;
    color: #f3f4f6;
    font-family: inherit;
    font-size: 20px;
    font-weight: 800;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  .mod-copy {
    min-width: 0;
    display: grid;
    gap: 4px;
  }

  .mod-copy strong {
    color: #f3f4f6;
    font-size: 13px;
  }

  .mod-copy span {
    color: #94a3b8;
    font-size: 10px;
  }

  .mod-copy p {
    margin: 2px 0 0;
    color: #94a3b8;
    font-size: 12px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .access-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin-top: 14px;
  }

  .access-options button {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 72px;
    padding: 14px 18px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    background: #181b25;
    color: #f3f4f6;
    text-align: left;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .access-options button:hover {
    background: #1f2333;
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-1px);
  }

  .access-options button.chosen {
    border-color: #22c55e;
    box-shadow: 0 0 18px rgba(34, 197, 94, 0.16), inset 0 0 0 1px #22c55e;
    background: rgba(34, 197, 94, 0.08);
  }

  .option-indicator {
    width: 18px;
    height: 18px;
    flex: 0 0 18px;
    border: 2px solid #4b5563;
    border-radius: 50%;
    background: transparent;
    transition: all 0.2s ease;
  }

  .chosen .option-indicator {
    border: 5px solid #22c55e;
    background: #ffffff;
    box-shadow: 0 0 8px rgba(34, 197, 94, 0.45);
  }

  .option-copy {
    display: grid;
    gap: 3px;
  }

  .option-copy strong {
    font-size: 14px;
    font-weight: 700;
    color: #f9fafb;
  }

  .option-copy small {
    color: #94a3b8;
    font-size: 12px;
  }

  .chosen .option-copy small {
    color: #86efac;
  }

  .form-footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px;
    padding-top: 22px;
  }

  .save-button {
    min-height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 0 20px;
    border: 0;
    border-radius: 8px;
    background: #22c55e;
    color: #0b1e13;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    box-shadow: 0 2px 10px rgba(34, 197, 94, 0.25);
    transition: all 0.15s ease;
  }

  .save-button:hover:not(:disabled) {
    background: #16a34a;
    box-shadow: 0 4px 16px rgba(34, 197, 94, 0.35);
    transform: translateY(-1px);
  }

  .save-button:disabled {
    opacity: 0.65;
    cursor: wait;
  }

  .save-button span {
    font-size: 18px;
    line-height: 1;
  }

  .saved-message,
  .feedback {
    margin: 0;
    color: #14866a;
    font-size: 12px;
  }

  .feedback {
    padding: 20px 0;
    color: #53625b;
  }

  .feedback.error {
    color: #a9362d;
  }

  /* File Explorer */
  .explorer-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
    padding: 10px 14px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
  }

  .path-breadcrumbs {
    display: flex;
    align-items: center;
    gap: 8px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 13px;
  }

  .breadcrumb-btn {
    border: 0;
    background: transparent;
    font-weight: 700;
    cursor: pointer;
    padding: 3px 8px;
    color: #22c55e;
    border-radius: 4px;
    font-size: 14px;
    transition: background 0.15s ease;
  }

  .breadcrumb-btn:hover:not(:disabled) {
    background: rgba(34, 197, 94, 0.12);
  }

  .breadcrumb-btn:disabled {
    color: #64748b;
    cursor: default;
  }

  .path-display {
    color: #f3f4f6;
    font-weight: 600;
  }

  .explorer-layout {
    display: grid;
    grid-template-columns: minmax(240px, 320px) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }

  .file-browser-pane {
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 16px;
    max-height: 520px;
    overflow-y: auto;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .file-browser-pane h3,
  .editor-header h3 {
    margin: 0 0 12px;
    font-size: 14px;
    font-weight: 700;
    color: #ffffff;
  }

  .file-tree {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file-tree-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid transparent;
    transition: background 0.15s ease;
  }

  .file-tree-item:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .file-tree-item.selected {
    background: rgba(34, 197, 94, 0.12);
    border-color: #22c55e;
  }

  .file-select-btn {
    flex: 1 1 auto;
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    text-align: left;
    min-width: 0;
    font-family: inherit;
    font-size: 12px;
  }

  .entry-icon {
    font-size: 14px;
    flex-shrink: 0;
  }

  .entry-name {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    color: #f3f4f6;
  }

  .entry-size {
    font-size: 10px;
    color: #94a3b8;
    flex-shrink: 0;
    font-family: monospace;
  }

  .file-delete-btn {
    border: 0;
    background: transparent;
    color: #f87171;
    font-size: 16px;
    font-weight: 700;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
    border-radius: 4px;
  }

  .file-delete-btn:hover {
    background: rgba(239, 68, 68, 0.15);
  }

  .file-editor-pane {
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 16px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }

  .file-editor-area {
    width: 100%;
    box-sizing: border-box;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 12px;
    line-height: 1.5;
    padding: 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: #11131a;
    color: #f3f4f6;
    resize: vertical;
  }

  .empty-editor-prompt,
  .loading-state,
  .empty-state {
    color: #94a3b8;
    font-size: 12px;
    font-style: italic;
    padding: 16px 0;
    margin: 0;
  }

  /* Players Management */
  .players-layout {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 20px;
    align-items: start;
  }

  .player-card {
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 18px;
    margin: 0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }

  .player-card legend {
    font-weight: 700;
    font-size: 14px;
    color: #f3f4f6;
    padding: 0 6px;
  }

  .player-add-row {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    margin-bottom: 14px;
  }

  .player-add-row .form-field {
    flex: 1 1 auto;
    margin: 0;
  }

  .player-entry-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }

  .player-entry-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .player-entry-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .player-entry-info strong {
    font-size: 13px;
    color: #f3f4f6;
  }

  .player-entry-info small {
    font-size: 10px;
    color: #94a3b8;
    font-family: monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .danger-button {
    background: #ef4444;
    color: #ffffff;
    border: 0;
    padding: 8px 14px;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    border-radius: 6px;
    transition: all 0.15s ease;
  }

  .danger-button:hover:not(:disabled) {
    background: #dc2626;
  }

  .danger-button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .save-button.small,
  .danger-button.small,
  .secondary-button.small {
    padding: 6px 10px;
    font-size: 11px;
  }

  /* Layout: App body & Sticky Left Servers Panel */
  .app-body {
    display: flex;
    height: calc(100vh - 72px);
    overflow: hidden;
  }

  .sidebar-servers {
    width: 310px;
    min-width: 280px;
    max-width: 340px;
    height: 100%;
    flex-shrink: 0;
    background: #1c1e22;
    border-right: 1px solid #282b32;
    display: flex;
    flex-direction: column;
    position: relative;
    z-index: 20;
  }

  .main-workspace {
    flex: 1;
    height: 100%;
    overflow-y: auto;
    min-width: 0;
  }

  /* Topbar Server Badge */
  .topbar-left {
    display: flex;
    align-items: center;
    gap: 18px;
  }

  .topbar-server-badge {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    font-size: 12px;
    color: #f3f4f6;
  }

  /* Dark Servers Panel Card matching user design */
  .servers-panel-card {
    width: 100%;
    height: 100%;
    background: #1c1e22;
    border: none;
    border-radius: 0;
    box-shadow: none;
    display: flex;
    flex-direction: column;
    color: #e5e7eb;
    position: relative;
  }

  .servers-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    background: #1c1e22;
    border-bottom: 1px solid #282b32;
  }

  .header-left,
  .header-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .rack-icon {
    flex-shrink: 0;
  }

  .servers-title {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: #ffffff;
    letter-spacing: -0.01em;
  }

  .server-tool-btn {
    background: transparent;
    border: 0;
    color: #d1d5db;
    font-size: 16px;
    font-weight: 700;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }

  .server-tool-btn:hover {
    background: #2b2e35;
    color: #ffffff;
  }

  .server-inline-create {
    margin: 10px;
    padding: 12px;
    background: #23262d;
    border: 1px solid #353942;
    border-radius: 10px;
  }

  .server-inline-create legend {
    color: #9ca3af;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0 4px;
  }

  .server-create-inputs {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 10px;
  }

  .form-field-dark {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .form-field-dark span {
    font-size: 11px;
    color: #9ca3af;
    font-weight: 600;
  }

  .form-field-dark input,
  .form-field-dark select {
    background: #181a1e;
    border: 1px solid #374151;
    border-radius: 6px;
    color: #f3f4f6;
    padding: 6px 10px;
    font-size: 12px;
    outline: none;
  }

  .form-field-dark input:focus,
  .form-field-dark select:focus {
    border-color: #3b82f6;
  }

  .server-create-actions {
    display: flex;
    gap: 8px;
  }

  .servers-card-list {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .server-item-card {
    display: flex;
    align-items: center;
    gap: 12px;
    background: #272a31;
    border: 1px solid #323640;
    border-radius: 10px;
    padding: 10px 12px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, transform 0.1s ease;
    user-select: none;
  }

  .server-item-card:hover {
    background: #2e323b;
    border-color: #434855;
    transform: translateY(-1px);
  }

  .server-item-card.active {
    background: #2a2e38;
    border-color: #4b6bfb;
    box-shadow: 0 0 0 1px #4b6bfb, 0 4px 12px rgba(75, 107, 251, 0.15);
  }

  .server-item-avatar {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #ffffff;
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.04em;
    flex-shrink: 0;
    box-shadow: inset 0 1px 2px rgba(255, 255, 255, 0.2), 0 2px 4px rgba(0, 0, 0, 0.3);
  }

  .server-item-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .server-item-name {
    color: #ffffff;
    font-size: 14px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .server-item-meta {
    color: #9ca3af;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .server-item-status {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  .server-status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }

  .server-status-dot.dot-running {
    background: #22c55e;
    box-shadow: 0 0 8px rgba(34, 197, 94, 0.6);
  }

  .server-status-dot.dot-crashed {
    background: #ef4444;
    box-shadow: 0 0 8px rgba(239, 68, 68, 0.6);
  }

  .server-status-dot.dot-stopped {
    background: #6b7280;
  }

  .servers-empty {
    padding: 24px 12px;
    text-align: center;
    color: #9ca3af;
  }

  .servers-empty p {
    margin: 0 0 12px;
    font-size: 13px;
  }

  /* Telemetry & Monitor Dashboard */
  .monitor-status-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 20px 24px;
    margin-bottom: 24px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.25);
  }

  .monitor-server-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .monitor-server-info h3 {
    margin: 2px 0 4px;
    font-size: 20px;
    font-weight: 700;
    color: #ffffff;
  }

  .monitor-controls {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .monitor-controls button {
    min-height: 38px;
    min-width: 130px;
    padding: 0 18px;
    font-size: 12px;
    font-weight: 700;
    border-radius: 8px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
  }

  .metrics-alert {
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    color: #f87171;
    border-radius: 8px;
    padding: 12px 16px;
    border-radius: 4px;
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 20px;
  }

  /* Resource Telemetry Divider */
  .monitor-section-divider {
    display: flex;
    align-items: center;
    gap: 16px;
    margin: 32px 0 20px;
    padding-top: 4px;
  }

  .divider-label {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .divider-label h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: #ffffff;
    font-family: inherit;
  }

  .divider-icon {
    color: #10b981;
    flex-shrink: 0;
  }

  .divider-line {
    flex: 1;
    height: 1px;
    background: linear-gradient(90deg, rgba(255, 255, 255, 0.14) 0%, rgba(255, 255, 255, 0.04) 100%);
  }

  .telemetry-live-badge {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 4px 11px;
    border-radius: 999px;
    background: rgba(16, 185, 129, 0.1);
    border: 1px solid rgba(16, 185, 129, 0.25);
    color: #34d399;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .live-pulse-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #10b981;
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.7);
  }

  .telemetry-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 18px;
    margin-bottom: 36px;
  }

  .telemetry-card {
    background: #171922;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.3), inset 0 1px 0 rgba(255, 255, 255, 0.05);
    transition: all 0.18s ease;
  }

  .telemetry-card:hover {
    border-color: rgba(255, 255, 255, 0.15);
    transform: translateY(-1px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }

  .telemetry-card.warning-card {
    border-color: rgba(245, 158, 11, 0.4);
    background: #1e1b15;
  }

  .telemetry-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .telemetry-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: #9ca3af;
  }

  .telemetry-badge {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #9ca3af;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .telemetry-badge.healthy {
    background: rgba(16, 185, 129, 0.15);
    color: #10b981;
    border-color: rgba(16, 185, 129, 0.3);
  }

  .telemetry-badge.warning {
    background: rgba(239, 68, 68, 0.15);
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.3);
  }

  .telemetry-value {
    font-size: 26px;
    font-weight: 800;
    color: #f3f4f6;
    line-height: 1.1;
    font-feature-settings: 'tnum';
    display: flex;
    align-items: baseline;
    gap: 4px;
  }

  .telemetry-unit {
    font-size: 13px;
    font-weight: 600;
    color: #9ca3af;
  }

  /* Telemetry Live Graph Box */
  .telemetry-graph-box {
    width: 100%;
    background: rgba(0, 0, 0, 0.28);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 6px 8px 5px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    box-shadow: inset 0 2px 6px rgba(0, 0, 0, 0.4);
  }

  .telemetry-svg {
    width: 100%;
    height: 65px;
    overflow: visible;
  }

  .graph-legend {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10px;
    color: #6b7280;
    padding: 0 2px;
  }

  .graph-legend strong {
    color: #d1d5db;
  }

  .legend-target {
    color: #9ca3af;
    font-weight: 600;
  }

  .telemetry-meter {
    height: 5px;
    background: rgba(255, 255, 255, 0.07);
    border-radius: 999px;
    overflow: hidden;
  }

  .telemetry-bar {
    height: 100%;
    border-radius: 999px;
    transition: width 0.3s ease;
  }

  .telemetry-bar.tps-bar {
    background: #10b981;
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.5);
  }

  .telemetry-bar.cpu-bar {
    background: #38bdf8;
    box-shadow: 0 0 8px rgba(56, 189, 248, 0.5);
  }

  .telemetry-bar.ram-bar {
    background: #a855f7;
    box-shadow: 0 0 8px rgba(168, 85, 247, 0.5);
  }

  .telemetry-bar.heap-bar {
    background: #f59e0b;
    box-shadow: 0 0 8px rgba(245, 158, 11, 0.5);
  }

  .telemetry-bar.alloc-bar {
    background: #10b981;
  }

  .telemetry-bar.danger {
    background: #ef4444;
    box-shadow: 0 0 8px rgba(239, 68, 68, 0.5);
  }

  .telemetry-subtext {
    font-size: 11px;
    color: #9ca3af;
    line-height: 1.3;
  }

  .telemetry-bar.alloc-bar {
    background: #059669;
  }

  .telemetry-bar.danger {
    background: #dc2626;
  }

  .telemetry-subtext {
    font-size: 11px;
    color: #6b7280;
    line-height: 1.3;
  }

  .text-danger {
    color: #dc2626;
    font-weight: 600;
  }

  .monitor-console-card {
    background: #111317;
    border: 1px solid #232730;
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
    display: flex;
    flex-direction: column;
    margin-bottom: 8px;
  }

  .monitor-console-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    background: #181b22;
    border-bottom: 1px solid #282d38;
  }

  .console-title-group {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .console-title-group h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 700;
    color: #e5e7eb;
    letter-spacing: 0.02em;
  }

  .console-count-pill {
    font-size: 10px;
    background: #262a34;
    color: #9ca3af;
    padding: 2px 7px;
    border-radius: 10px;
    font-weight: 600;
  }

  .monitor-terminal {
    height: 320px;
    overflow-y: auto;
    padding: 12px 16px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.5;
    background: #0d0f13;
    color: #d1d5db;
  }

  .terminal-empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: #6b7280;
    text-align: center;
  }

  .terminal-empty p {
    margin: 0 0 4px;
    font-size: 13px;
  }

  .terminal-lines {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .terminal-line {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    word-break: break-all;
  }

  .terminal-tag {
    font-size: 10px;
    font-weight: 800;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(59, 130, 246, 0.2);
    color: #60a5fa;
    flex-shrink: 0;
  }

  .terminal-tag.tag-warn {
    background: rgba(245, 158, 11, 0.2);
    color: #fbbf24;
  }

  .terminal-tag.tag-error {
    background: rgba(239, 68, 68, 0.2);
    color: #f87171;
  }

  .terminal-msg {
    color: #e5e7eb;
  }

  .terminal-line.line-warn .terminal-msg {
    color: #fef08a;
  }

  .terminal-line.line-error .terminal-msg {
    color: #fca5a5;
  }

  .monitor-cmd-row {
    display: flex;
    padding: 10px 14px;
    background: #181b22;
    border-top: 1px solid #282d38;
    gap: 10px;
  }

  .monitor-cmd-row input {
    flex: 1;
    background: #0d0f13;
    border: 1px solid #374151;
    border-radius: 4px;
    color: #f3f4f6;
    padding: 8px 12px;
    font-family: inherit;
    font-size: 12px;
    outline: none;
  }

  .monitor-cmd-row input:focus {
    border-color: #3b82f6;
  }

  @media (max-width: 520px) {
    .topbar {
      height: 62px;
      padding: 0 20px;
    }

    .setup-view {
      width: calc(100% - 40px);
      padding: 48px 0 64px;
    }

    h1 {
      font-size: 40px;
    }

    .access-options {
      grid-template-columns: 1fr;
    }

    .workspace-tabs {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .workspace-tabs button {
      min-height: 44px;
      font-size: 10px;
    }

    .explorer-layout,
    .players-layout {
      grid-template-columns: 1fr;
    }

    .server-status {
      display: grid;
    }

    .installed-mod-row {
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .installed-mod-row .remove-action {
      grid-column: 2;
    }

    .mod-result {
      grid-template-columns: 40px minmax(0, 1fr);
      grid-template-rows: auto auto;
      align-items: flex-start;
      gap: 10px;
      padding: 10px 12px;
    }

    .mod-result img,
    .mod-mark {
      width: 40px;
      height: 40px;
    }

    .install-mod-button,
    .external-link-btn {
      grid-column: 1 / -1;
      width: 100%;
      min-height: 32px;
      margin-top: 4px;
      justify-content: center;
      text-align: center;
    }

    .address-fields {
      grid-template-columns: minmax(80px, 0.8fr) 8px minmax(0, 1.2fr);
      gap: 6px;
    }
  }

  .remote-card {
    margin-top: 24px;
    padding: 22px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.3);
  }

  .remote-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: 16px;
  }

  .remote-title-group h3 {
    margin: 4px 0 6px 0;
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: #ffffff;
  }

  .remote-top-actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .pairing-ticket-box {
    margin-top: 18px;
    padding: 18px;
    background: #10121a;
    border: 1px dashed rgba(34, 197, 94, 0.35);
    border-radius: 12px;
  }

  .ticket-qr-section {
    display: flex;
    align-items: center;
    gap: 20px;
    flex-wrap: wrap;
  }

  .ticket-info {
    flex: 1;
    min-width: 200px;
  }

  .ticket-code-display {
    margin: 10px 0;
  }

  .ticket-code-display strong {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 24px;
    font-weight: 800;
    letter-spacing: 0.18em;
    background: #1a1d28;
    color: #22c55e;
    padding: 8px 16px;
    border-radius: 8px;
    border: 1px solid rgba(34, 197, 94, 0.25);
    display: inline-block;
  }

  .paired-devices-section {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }

  .paired-device-list {
    list-style: none;
    margin: 10px 0 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .paired-device-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 14px;
    background: #141620;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
  }

  .paired-device-info {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: #f3f4f6;
  }

  .device-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .device-dot.dot-online {
    background: #22c55e;
    box-shadow: 0 0 8px rgba(34, 197, 94, 0.5);
  }

  .device-badge {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 7px;
    background: rgba(34, 197, 94, 0.15);
    color: #4ade80;
    border-radius: 4px;
    border: 1px solid rgba(34, 197, 94, 0.25);
  }

  .mobile-shell {
    min-height: 100vh;
    max-width: 520px;
    margin: 0 auto;
    background: #16181d;
    color: #f1f3ed;
    display: flex;
    flex-direction: column;
    box-shadow: 0 0 48px rgba(0, 0, 0, 0.6);
    position: relative;
    padding-bottom: 76px;
  }

  .mobile-header {
    background: #1c1e24;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    padding: 14px 16px;
    position: sticky;
    top: 0;
    z-index: 50;
  }

  .mobile-header-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .mobile-brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .mobile-brand-text {
    display: flex;
    flex-direction: column;
    line-height: 1.1;
  }

  .mobile-brand-text strong {
    font-size: 13px;
    font-weight: 900;
    letter-spacing: 0.08em;
  }

  .mobile-brand-text small {
    font-size: 9px;
    font-weight: 700;
    color: #df563f;
    letter-spacing: 0.06em;
  }

  .mobile-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .iroh-badge {
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.04em;
    color: #22c55e;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.25);
    padding: 4px 8px;
    border-radius: 999px;
  }

  .iroh-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #22c55e;
  }

  .mobile-exit-btn {
    min-height: 36px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 700;
  }

  .mobile-status-strip {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .mobile-instance-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .mobile-ver {
    font-size: 11px;
    color: #9ca3af;
  }

  .mobile-content {
    flex: 1;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .mobile-card {
    background: #20232a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 16px;
  }

  .mobile-card h4 {
    margin: 0 0 12px 0;
    font-size: 11px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #9ca3af;
  }

  .mobile-action-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .mobile-action-header h4 {
    margin: 0;
  }

  .mobile-refresh-btn {
    min-height: 38px;
    padding: 0 12px;
    font-size: 12px;
    font-weight: 700;
  }

  .mobile-primary-btn {
    width: 100%;
    min-height: 52px;
    font-size: 15px;
    font-weight: 800;
    letter-spacing: 0.02em;
    border-radius: 10px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .restart-accent {
    background: #d97706 !important;
  }

  .mobile-telemetry-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 10px;
  }

  .mobile-telemetry-item {
    background: #181a1f;
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 10px;
    padding: 12px;
  }

  .mobile-telemetry-item .stat-kicker {
    font-size: 9px;
    font-weight: 800;
    color: #9ca3af;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .mobile-telemetry-item .stat-value {
    font-size: 20px;
    font-weight: 900;
    color: #f1f3ed;
    margin: 4px 0 2px 0;
  }

  .mobile-telemetry-item .stat-sub {
    font-size: 10px;
    color: #6b7280;
  }

  .mobile-process-info {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: #9ca3af;
    font-family: monospace;
  }

  .mobile-join-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .mobile-join-code {
    flex: 1;
    font-family: monospace;
    font-size: 12px;
    background: #181a1f;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    overflow-x: auto;
    white-space: nowrap;
  }

  .mobile-terminal {
    background: #0d0f12;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    padding: 10px;
    height: 240px;
    overflow-y: auto;
    font-family: monospace;
    font-size: 11px;
    line-height: 1.5;
  }

  .mobile-console-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }

  .mobile-console-header h4 {
    margin: 0;
  }

  .mobile-cmd-form {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }

  .mobile-cmd-input {
    flex: 1;
    min-height: 44px;
    background: #181a1f;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: #f1f3ed;
    padding: 0 12px;
    font-family: monospace;
    font-size: 13px;
  }

  .mobile-cmd-submit {
    min-height: 44px;
    min-width: 68px;
    font-weight: 800;
    border-radius: 8px;
  }

  .mobile-quick-cmds {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 8px;
  }

  .quick-cmd-chip {
    background: rgba(255, 255, 255, 0.08);
    border: none;
    color: #d1d5db;
    padding: 6px 10px;
    border-radius: 6px;
    font-size: 11px;
    font-family: monospace;
    cursor: pointer;
    min-height: 32px;
  }

  .mobile-player-add-form {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .mobile-player-add-form input {
    flex: 1;
    min-height: 44px;
    background: #181a1f;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: #f1f3ed;
    padding: 0 12px;
    font-size: 13px;
  }

  .mobile-player-add-form button {
    min-height: 44px;
    min-width: 64px;
    border-radius: 8px;
  }

  .mobile-player-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .mobile-player-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 12px;
    background: #181a1f;
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
  }

  .mobile-player-item .player-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mobile-player-item .player-info strong {
    font-size: 13px;
    color: #f1f3ed;
  }

  .mobile-player-item .player-info small {
    font-size: 10px;
    color: #6b7280;
    font-family: monospace;
  }

  .mobile-bottom-nav {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    max-width: 520px;
    margin: 0 auto;
    background: #1c1e24;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    height: 64px;
    z-index: 100;
  }

  .mobile-nav-item {
    min-height: 44px;
    height: 100%;
    background: none;
    border: none;
    color: #9ca3af;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .mobile-nav-item.active {
    color: #df563f;
    font-weight: 800;
  }

  .mobile-nav-item .nav-icon {
    font-size: 18px;
  }

  .mobile-nav-item .nav-label {
    font-size: 10px;
    letter-spacing: 0.04em;
    font-weight: 700;
  }

  .mobile-touch-button {
    width: 100%;
    min-height: 48px;
    font-size: 14px;
    font-weight: 700;
    border-radius: 8px;
  }

  .server-sidebar-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 12px 14px 14px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    margin-top: auto;
  }

  .sidebar-action-btn {
    min-height: 34px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    background: rgba(255, 255, 255, 0.06);
    color: #e5e7eb;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .sidebar-action-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.28);
  }

  .sidebar-action-btn.danger {
    color: #fca5a5;
    border-color: rgba(239, 68, 68, 0.35);
  }

  .sidebar-action-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.16);
    border-color: rgba(239, 68, 68, 0.55);
  }

  .sidebar-action-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .provisioning-card {
    margin: 22px 0 8px;
    padding: 18px;
    border-radius: 12px;
    background: #181b25;
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .provisioning-header h3 {
    margin: 4px 0 6px;
    color: #ffffff;
    font-family: inherit;
    font-size: 18px;
    font-weight: 700;
  }

  .provisioning-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 12px;
  }

  .provision-step {
    padding: 12px;
    border-radius: 8px;
    background: #11131a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .provision-step-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .provision-step-top strong {
    font-size: 12px;
    color: #f3f4f6;
  }

  .provision-link {
    color: #22c55e;
    font-weight: 700;
    font-size: 12px;
    text-decoration: underline;
  }

  .eula-checkbox-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 11px;
    line-height: 1.4;
    color: #f3f4f6;
    cursor: pointer;
  }

  .eula-checkbox-row input {
    margin-top: 2px;
  }

  .crash-report-box {
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: 8px;
    background: rgba(239, 68, 68, 0.08);
    padding: 10px 12px;
  }

  .crash-report-box summary {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-size: 12px;
    color: #fca5a5;
  }

  .crash-badge {
    display: inline-block;
    padding: 2px 6px;
    border-radius: 3px;
    background: #df563f;
    color: #fffaf1;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.06em;
  }

  .crash-report-pre {
    margin: 10px 0 0;
    max-height: 220px;
    overflow: auto;
    padding: 10px;
    border-radius: 4px;
    background: #1c1e22;
    color: #f1f3ed;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    line-height: 1.45;
    white-space: pre-wrap;
  }

  /* ==========================================================================
     Ultra-Responsive Design: Desktop, Tablet, Mobile Breakpoints
     ========================================================================== */
  @media (max-width: 1080px) {
    .workspace-tabs {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 6px;
    }

    .catalog-category-sidebar {
      width: 180px;
    }

    .catalog-category-sidebar.collapsed {
      width: 130px;
    }

    .telemetry-grid {
      grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
      gap: 12px;
    }
  }

  @media (max-width: 768px) {
    .app-body {
      flex-direction: column;
      height: auto;
      min-height: calc(100vh - 72px);
      overflow-y: auto;
    }

    .sidebar-servers {
      width: 100%;
      min-width: 100%;
      max-width: 100%;
      height: auto;
      max-height: 280px;
      border-right: none;
      border-bottom: 1px solid #282b32;
    }

    .main-workspace {
      height: auto;
      overflow: visible;
    }

    .setup-view {
      width: calc(100% - 24px);
      padding: 24px 0 48px;
    }

    .catalog-category-sidebar {
      width: 175px;
    }

    .catalog-category-sidebar.collapsed {
      width: 120px;
    }

    .catalog-controls-top-row {
      gap: 8px;
    }

    .monitor-status-header {
      flex-direction: column;
      align-items: flex-start;
      gap: 16px;
    }

    .monitor-controls {
      width: 100%;
      justify-content: flex-start;
      flex-wrap: wrap;
    }

    .monitor-controls button {
      flex: 1;
    }

    .join-address-box {
      flex-direction: column;
      align-items: flex-start;
      gap: 12px;
    }

    .join-actions {
      width: 100%;
      justify-content: space-between;
    }

    .join-actions button {
      flex: 1;
    }

    .explorer-layout,
    .players-layout {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 480px) {
    .topbar {
      height: 58px;
      padding: 0 14px;
    }

    .workspace-tabs {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 4px;
    }

    .workspace-tabs button {
      font-size: 11px;
      padding: 6px 6px;
    }

    .access-options {
      grid-template-columns: 1fr;
    }

    .server-sidebar-actions {
      grid-template-columns: 1fr;
    }

    .props-grid-3,
    .props-grid-4,
    .props-toggles-grid {
      grid-template-columns: 1fr;
    }

    .catalog-workbench-layout {
      flex-direction: column;
      gap: 10px;
    }

    .catalog-category-sidebar {
      width: 100%;
    }

    .catalog-category-sidebar.collapsed {
      width: 100%;
    }

    .category-drawer-list {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr)) !important;
      max-height: 180px;
    }

    .env-toggle-group {
      width: 100%;
      display: flex;
    }

    .env-toggle-btn {
      flex: 1;
      padding: 4px 8px;
      font-size: 11px;
    }
  }

  /* ==========================================================================
     Light Theme Mode Styling (Functional, High-Contrast, Polished)
     ========================================================================== */
  :global(html[data-theme="light"]),
  :global(html[data-theme="light"] body) {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .app-shell {
    background: #f1f5f9;
    background-image: radial-gradient(circle at 50% 0%, rgba(27, 217, 106, 0.08) 0%, transparent 60%);
  }

  :global(html[data-theme="light"]) .topbar {
    background: rgba(255, 255, 255, 0.95);
    border-bottom: 1px solid #e2e8f0;
  }

  :global(html[data-theme="light"]) .brand {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .sidebar-servers {
    background: #ffffff;
    border-right: 1px solid #e2e8f0;
  }

  :global(html[data-theme="light"]) .servers-panel-card,
  :global(html[data-theme="light"]) .servers-panel-header {
    background: #ffffff;
    border-bottom: 1px solid #e2e8f0;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .server-item-card {
    background: #f8fafc;
    border-color: #e2e8f0;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .server-item-card:hover {
    background: #f1f5f9;
  }

  :global(html[data-theme="light"]) .server-item-card.active {
    background: #ecfdf5;
    border-color: #10b981;
  }

  :global(html[data-theme="light"]) .server-item-name {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .workspace-tabs {
    background: #e2e8f0;
    border-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .workspace-tabs button {
    color: #475569;
  }

  :global(html[data-theme="light"]) .workspace-tabs button:hover {
    color: #0f172a;
    background: rgba(0, 0, 0, 0.04);
  }

  :global(html[data-theme="light"]) .workspace-tabs button.active {
    background: #ffffff;
    color: #0f172a;
    border-color: #cbd5e1;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  }

  :global(html[data-theme="light"]) .mod-catalog,
  :global(html[data-theme="light"]) .join-card,
  :global(html[data-theme="light"]) .remote-card,
  :global(html[data-theme="light"]) .telemetry-card,
  :global(html[data-theme="light"]) .monitor-status-header,
  :global(html[data-theme="light"]) .monitor-console-card,
  :global(html[data-theme="light"]) .file-browser-pane,
  :global(html[data-theme="light"]) .editor-pane,
  :global(html[data-theme="light"]) .preferences-dialog,
  :global(html[data-theme="light"]) .shortcuts-dialog,
  :global(html[data-theme="light"]) .about-dialog {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.06);
  }

  :global(html[data-theme="light"]) .panel-heading h2,
  :global(html[data-theme="light"]) .monitor-server-info h3,
  :global(html[data-theme="light"]) .join-card-header h3,
  :global(html[data-theme="light"]) .remote-title-group h3,
  :global(html[data-theme="light"]) #monitor-console-title,
  :global(html[data-theme="light"]) .file-browser-pane h3,
  :global(html[data-theme="light"]) .editor-header h3,
  :global(html[data-theme="light"]) .dialog-header h2,
  :global(html[data-theme="light"]) h1 {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .join-address-box,
  :global(html[data-theme="light"]) .explorer-toolbar,
  :global(html[data-theme="light"]) .catalog-controls-deck,
  :global(html[data-theme="light"]) .catalog-category-sidebar,
  :global(html[data-theme="light"]) .options-dropdown-menu {
    background: #f8fafc;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .dropdown-menu-item {
    color: #334155;
  }

  :global(html[data-theme="light"]) .dropdown-menu-item:hover {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .secondary-button {
    background: #f1f5f9;
    border-color: #cbd5e1;
    color: #334155;
  }

  :global(html[data-theme="light"]) .secondary-button:hover:not(:disabled) {
    background: #e2e8f0;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .curseforge-key-section {
    background: #ffffff;
    border-color: #fdba74;
  }

  :global(html[data-theme="light"]) .cf-key-hint-box {
    background: #fff7ed;
    border-color: #fed7aa;
  }

  :global(html[data-theme="light"]) .cf-key-hint-desc,
  :global(html[data-theme="light"]) .cf-key-hint-steps ol {
    color: #475569;
  }

  :global(html[data-theme="light"]) .password-input-row input {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  /* Status Pill in Light Theme */
  :global(html[data-theme="light"]) .status-pill {
    background: #f1f5f9;
    border-color: #cbd5e1;
    color: #1e293b;
    font-weight: 800;
  }

  :global(html[data-theme="light"]) .status-pill.running {
    background: #dcfce7;
    border-color: #86efac;
    color: #15803d;
  }

  :global(html[data-theme="light"]) .status-pill.crashed {
    background: #fee2e2;
    border-color: #fca5a5;
    color: #b91c1c;
  }

  /* Telemetry Cards & Metrics in Light Theme */
  :global(html[data-theme="light"]) .telemetry-card {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.05);
  }

  :global(html[data-theme="light"]) .telemetry-card:hover {
    border-color: #cbd5e1;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
  }

  :global(html[data-theme="light"]) .telemetry-label {
    color: #475569;
  }

  :global(html[data-theme="light"]) .telemetry-value {
    color: #0f172a;
    font-weight: 800;
  }

  :global(html[data-theme="light"]) .telemetry-subtext {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .graph-legend {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .graph-legend strong {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .legend-target {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .telemetry-meter {
    background: #e2e8f0;
  }

  :global(html[data-theme="light"]) .divider-label h3 {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .divider-line {
    background: linear-gradient(90deg, #cbd5e1 0%, #e2e8f0 100%);
  }

  :global(html[data-theme="light"]) .telemetry-live-badge {
    background: #ecfdf5;
    border-color: #a7f3d0;
    color: #059669;
  }

  /* Network Access & Join Address in Light Theme */
  :global(html[data-theme="light"]) .access-options button {
    background: #ffffff;
    border-color: #e2e8f0;
    color: #0f172a;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  }

  :global(html[data-theme="light"]) .access-options button:hover {
    background: #f8fafc;
    border-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .access-options button.chosen {
    background: #f0fdf4;
    border-color: #10b981;
    box-shadow: 0 0 16px rgba(16, 185, 129, 0.15), inset 0 0 0 1px #10b981;
  }

  :global(html[data-theme="light"]) .option-copy strong {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .option-copy small {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .chosen .option-copy small {
    color: #15803d;
    font-weight: 600;
  }

  :global(html[data-theme="light"]) .option-indicator {
    border-color: #94a3b8;
  }

  :global(html[data-theme="light"]) .chosen .option-indicator {
    border-color: #10b981;
    background: #ffffff;
  }

  :global(html[data-theme="light"]) .join-card {
    background: #ffffff;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .join-card-header h3 {
    color: #0f172a;
    font-family: inherit;
    font-weight: 700;
  }

  :global(html[data-theme="light"]) .join-address-box {
    background: #f8fafc;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .join-address-text {
    color: #15803d;
  }

  :global(html[data-theme="light"]) .join-port {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .join-loading {
    color: #64748b;
  }

  /* File Explorer in Light Theme */
  :global(html[data-theme="light"]) .explorer-toolbar {
    background: #ffffff;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .breadcrumb-btn {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .breadcrumb-btn:hover {
    background: #f1f5f9;
  }

  :global(html[data-theme="light"]) .file-tree-item {
    color: #334155;
  }

  :global(html[data-theme="light"]) .file-tree-item:hover {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .file-tree-item.selected {
    background: #e0f2fe;
    color: #0369a1;
  }

  :global(html[data-theme="light"]) .file-editor-textarea {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  /* Typography & Forms in Light Theme */
  :global(html[data-theme="light"]) .intro h1,
  :global(html[data-theme="light"]) .panel-heading h2 {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .intro p,
  :global(html[data-theme="light"]) .panel-heading p {
    color: #475569;
  }

  :global(html[data-theme="light"]) .field-hint {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .step-label {
    color: #64748b;
  }

  :global(html[data-theme="light"]) legend {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .form-field input,
  :global(html[data-theme="light"]) .form-field select,
  :global(html[data-theme="light"]) .form-field textarea {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .form-field span {
    color: #334155;
  }

  /* Project Type Selector & Catalog Source in Light Theme */
  :global(html[data-theme="light"]) .project-type-selector {
    background: #e2e8f0;
    border-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .project-type-pill {
    color: #475569;
  }

  :global(html[data-theme="light"]) .project-type-pill:hover:not(:disabled) {
    background: rgba(0, 0, 0, 0.05);
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .project-type-pill.active {
    background: #ffffff;
    color: #0f172a;
    border-color: #cbd5e1;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
    font-weight: 700;
  }

  :global(html[data-theme="light"]) .catalog-source-selector {
    background: #e2e8f0;
    border-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .catalog-source-btn {
    color: #475569;
  }

  :global(html[data-theme="light"]) .catalog-source-btn:hover {
    background: rgba(0, 0, 0, 0.05);
    color: #0f172a;
  }

  /* Environment Toggle in Light Theme */
  :global(html[data-theme="light"]) .env-toggle-group {
    background: #e2e8f0;
    border-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .env-toggle-btn {
    color: #475569;
  }

  :global(html[data-theme="light"]) .env-toggle-btn:hover:not(:disabled) {
    background: rgba(0, 0, 0, 0.05);
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .env-toggle-btn.active {
    background: #ffffff;
    color: #0f172a;
    border-color: #cbd5e1;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.08);
  }

  :global(html[data-theme="light"]) .theme-modrinth .env-toggle-btn.active {
    background: #ecfdf5;
    color: #047857;
    border-color: #a7f3d0;
  }

  :global(html[data-theme="light"]) .theme-curseforge .env-toggle-btn.active {
    background: #fff7ed;
    color: #c2410c;
    border-color: #fed7aa;
  }

  /* Category Drawer in Light Theme */
  :global(html[data-theme="light"]) .category-sidebar-header-btn {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .category-drawer-item {
    color: #334155;
  }

  :global(html[data-theme="light"]) .category-drawer-item:hover:not(:disabled) {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .category-drawer-item.active {
    background: #ecfdf5;
    color: #047857;
    font-weight: 700;
  }

  :global(html[data-theme="light"]) .category-drawer-list::-webkit-scrollbar-thumb {
    background: #cbd5e1;
  }

  /* Mod Cards in Light Theme */
  :global(html[data-theme="light"]) .mod-result {
    background: #ffffff;
    border-color: #e2e8f0;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
  }

  :global(html[data-theme="light"]) .mod-result:hover {
    border-color: #cbd5e1;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
  }

  :global(html[data-theme="light"]) .mod-title-text {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .mod-description {
    color: #475569;
  }

  :global(html[data-theme="light"]) .mod-meta-badge {
    background: #f1f5f9;
    color: #475569;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .catalog-search-field input {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .catalog-search-field input::placeholder {
    color: #94a3b8;
  }

  :global(html[data-theme="light"]) .sort-select {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .curseforge-locked-banner {
    background: #fff7ed;
    border-color: #fed7aa;
  }

  :global(html[data-theme="light"]) .locked-banner-content h4 {
    color: #9a3412;
  }

  :global(html[data-theme="light"]) .locked-banner-content p {
    color: #7c2d12;
  }

  /* Preferences Dialog in Light Theme */
  :global(html[data-theme="light"]) .pref-section-heading {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .pref-card-group {
    background: #f8fafc;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .pref-row-title {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .pref-row-subtitle {
    color: #64748b;
  }

  :global(html[data-theme="light"]) .pref-select {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .pref-stepper-control {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .stepper-count {
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .stepper-btn {
    color: #334155;
  }

  :global(html[data-theme="light"]) .stepper-btn:hover {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .pref-segmented-nav {
    background: #e2e8f0;
  }

  :global(html[data-theme="light"]) .pref-segment-btn {
    color: #475569;
  }

  :global(html[data-theme="light"]) .pref-segment-btn.active {
    background: #ffffff;
    color: #0f172a;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.08);
  }

  :global(html[data-theme="light"]) .pref-close-btn,
  :global(html[data-theme="light"]) .pref-tool-btn {
    color: #475569;
  }

  :global(html[data-theme="light"]) .pref-close-btn:hover,
  :global(html[data-theme="light"]) .pref-tool-btn:hover {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .switch-pill {
    background-color: #cbd5e1;
  }

  :global(html[data-theme="light"]) .pref-switch input:checked + .switch-pill {
    background-color: #2563eb;
  }

  :global(html[data-theme="light"]) .pref-arrow-btn {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #475569;
  }

  :global(html[data-theme="light"]) .pref-arrow-btn:hover {
    background: #f1f5f9;
    color: #0f172a;
  }

  :global(html[data-theme="light"]) .pref-sub-options-row {
    background: #f1f5f9;
    border-color: #e2e8f0;
  }

  :global(html[data-theme="light"]) .pref-radio-pill {
    background: #ffffff;
    border-color: #cbd5e1;
    color: #475569;
  }

  :global(html[data-theme="light"]) .pref-radio-pill.active {
    background: #eff6ff;
    color: #1d4ed8;
    border-color: #93c5fd;
  }
</style>
