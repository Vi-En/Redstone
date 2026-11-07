import { invoke, isTauri } from '@tauri-apps/api/core';
import { getNetworkSecret, saveNetworkSecret } from './network';

export type ProjectType = 'mod' | 'resourcepack' | 'datapack' | 'plugin';
export type EnvironmentFilter = 'all' | 'client' | 'server' | 'both';

export interface ModSearchResult {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  icon_url: string | null;
  source?: 'modrinth' | 'curseforge';
  project_url?: string | null;
  distribution_disabled?: boolean;
  categories?: string[];
  client_side?: 'required' | 'optional' | 'unsupported';
  server_side?: 'required' | 'optional' | 'unsupported';
  project_type?: ProjectType;
}

export interface ModSearchResponse {
  results: ModSearchResult[];
  previewOnly: boolean;
}

export interface ModInstallResponse {
  fileName: string;
  versionId: string;
  previewOnly: boolean;
}

export interface InstalledMod {
  file_name: string;
  enabled: boolean;
}

export interface InstalledModsResponse {
  mods: InstalledMod[];
  previewOnly: boolean;
}

export interface TrackedMod {
  project_id: string;
  file_name: string;
  version_id: string;
}

export interface ModUpdate {
  project_id: string;
  file_name: string;
  current_version_id: string;
  latest_version_id: string;
  latest_version_number: string;
  latest_file_name: string;
}

const previewCatalog: ModSearchResult[] = [
  {
    project_id: 'preview-sodium',
    slug: 'sodium',
    title: 'Sodium',
    description: 'A modern rendering engine and client-side optimization mod for Minecraft.',
    author: 'jellysquid3',
    downloads: 12500000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Optimization', 'Utility'],
    client_side: 'required',
    server_side: 'unsupported'
  },
  {
    project_id: 'preview-lithium',
    slug: 'lithium',
    title: 'Lithium',
    description: 'General-purpose game logic, physics, and chunk ticking optimizations for servers and clients.',
    author: 'jellysquid3',
    downloads: 9800000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Optimization', 'Technology'],
    client_side: 'optional',
    server_side: 'required'
  },
  {
    project_id: 'preview-ferritecore',
    slug: 'ferritecore',
    title: 'FerriteCore',
    description: 'Memory usage optimizations reducing RAM consumed by block states, models, and caches.',
    author: 'malte0811',
    downloads: 8400000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Optimization', 'Performance'],
    client_side: 'optional',
    server_side: 'optional'
  },
  {
    project_id: 'preview-appleskin',
    slug: 'appleskin',
    title: 'AppleSkin',
    description: 'Adds hunger and saturation HUD indicators to Minecraft.',
    author: 'squeek502',
    downloads: 15200000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Utility', 'Decoration'],
    client_side: 'required',
    server_side: 'optional'
  },
  {
    project_id: 'preview-ae2',
    slug: 'applied-energistics-2',
    title: 'Applied Energistics 2',
    description: 'A comprehensive energy, matter, and grid storage system automation mod.',
    author: 'AlgorithmX2',
    downloads: 24000000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Technology', 'Storage'],
    client_side: 'optional',
    server_side: 'required'
  },
  {
    project_id: 'preview-farmers-delight',
    slug: 'farmers-delight',
    title: "Farmer's Delight",
    description: 'Gently expands farming, cooking, and food preparation in Minecraft.',
    author: 'vectorwing',
    downloads: 18500000,
    icon_url: null,
    source: 'modrinth',
    categories: ['Adventure', 'Decoration'],
    client_side: 'optional',
    server_side: 'required'
  },
  // Resource Packs
  {
    project_id: 'preview-faithful',
    slug: 'faithful-32x',
    title: 'Faithful 32x',
    description: 'High-definition default textures preserving the vanilla Minecraft aesthetic.',
    author: 'FaithfulTeam',
    downloads: 32000000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'resourcepack',
    categories: ['Decoration', 'Utility'],
    client_side: 'required',
    server_side: 'unsupported'
  },
  {
    project_id: 'preview-bare-bones',
    slug: 'bare-bones',
    title: 'Bare Bones',
    description: 'Brings Minecraft graphics to the clean, vibrant look of the official trailers.',
    author: 'RobotPantaloons',
    downloads: 14500000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'resourcepack',
    categories: ['Decoration'],
    client_side: 'required',
    server_side: 'unsupported'
  },
  // Data Packs
  {
    project_id: 'preview-terralith',
    slug: 'terralith',
    title: 'Terralith',
    description: 'Massive overhaul of Minecraft world generation with nearly 100 brand new biomes.',
    author: 'Starmute',
    downloads: 16000000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'datapack',
    categories: ['World Generation', 'Adventure'],
    client_side: 'optional',
    server_side: 'required'
  },
  {
    project_id: 'preview-incendium',
    slug: 'incendium',
    title: 'Incendium',
    description: 'Nether expansion with customized biomes, dungeons, terrain, and custom items.',
    author: 'Starmute',
    downloads: 6500000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'datapack',
    categories: ['World Generation', 'Adventure', 'Cursed'],
    client_side: 'optional',
    server_side: 'required'
  },
  // Plugins
  {
    project_id: 'preview-essentialsx',
    slug: 'essentialsx',
    title: 'EssentialsX',
    description: 'Essential server management suite including warps, spawn, homes, economy, and moderating tools.',
    author: 'EssentialsX Team',
    downloads: 45000000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'plugin',
    categories: ['Management', 'Economy', 'Utility'],
    client_side: 'unsupported',
    server_side: 'required'
  },
  {
    project_id: 'preview-coreprotect',
    slug: 'coreprotect',
    title: 'CoreProtect',
    description: 'Fast, efficient block logging, rollback, and grief-prevention inspection tool.',
    author: 'Intelli',
    downloads: 28000000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'plugin',
    categories: ['Management', 'Utility'],
    client_side: 'unsupported',
    server_side: 'required'
  },
  {
    project_id: 'preview-vault',
    slug: 'vault',
    title: 'Vault',
    description: 'Permissions, chat, and economy bridge API connecting plugins together.',
    author: 'Sleaker',
    downloads: 38000000,
    icon_url: null,
    source: 'modrinth',
    project_type: 'plugin',
    categories: ['Economy', 'Library'],
    client_side: 'unsupported',
    server_side: 'required'
  }
];

const previewCurseForgeCatalog: ModSearchResult[] = [
  {
    project_id: 'curseforge-238222',
    slug: 'jei',
    title: 'Just Enough Items (JEI)',
    description: 'View Items, Recipes, and crafting ingredients seamlessly.',
    author: 'mezz',
    downloads: 250000000,
    icon_url: null,
    source: 'curseforge',
    project_url: 'https://www.curseforge.com/minecraft/mc-mods/jei',
    distribution_disabled: false,
    categories: ['Technology', 'Storage', 'Server Utility'],
    client_side: 'required',
    server_side: 'optional'
  },
  {
    project_id: 'curseforge-32274',
    slug: 'journeymap',
    title: 'JourneyMap',
    description: 'Real-time mapping in-game or in a web browser as you explore.',
    author: 'techbrew',
    downloads: 195000000,
    icon_url: null,
    source: 'curseforge',
    project_url: 'https://www.curseforge.com/minecraft/mc-mods/journeymap',
    distribution_disabled: false,
    categories: ['Adventure and RPG', 'Server Utility'],
    client_side: 'required',
    server_side: 'optional'
  },
  {
    project_id: 'curseforge-246644',
    slug: 'iron-chests',
    title: 'Iron Chests',
    description: 'Upgradable storage chests with tiered capacities (Copper, Iron, Gold, Diamond).',
    author: 'ProgWML6',
    downloads: 140000000,
    icon_url: null,
    source: 'curseforge',
    project_url: 'https://www.curseforge.com/minecraft/mc-mods/iron-chests',
    distribution_disabled: false,
    categories: ['Storage', 'Technology'],
    client_side: 'optional',
    server_side: 'required'
  },
  {
    project_id: 'curseforge-999999',
    slug: 'optifine-preview',
    title: 'OptiFine (Distribution Disabled)',
    description: 'Distribution disabled by author; direct download is unavailable.',
    author: 'sp614x',
    downloads: 80000000,
    icon_url: null,
    source: 'curseforge',
    project_url: 'https://www.curseforge.com/minecraft/mc-mods/optifine',
    distribution_disabled: true,
    categories: ['Performance & Fixes', 'Cosmetic & Decoration'],
    client_side: 'required',
    server_side: 'unsupported'
  },
  {
    project_id: 'curseforge-411234',
    slug: 'stay-true',
    title: 'Stay True',
    description: 'Visual remaster designed to improve the default art style without breaking it.',
    author: 'hai_matsumoto',
    downloads: 48000000,
    icon_url: null,
    source: 'curseforge',
    project_type: 'resourcepack',
    project_url: 'https://www.curseforge.com/minecraft/texture-packs/stay-true',
    distribution_disabled: false,
    categories: ['Decoration'],
    client_side: 'required',
    server_side: 'unsupported'
  },
  {
    project_id: 'curseforge-555123',
    slug: 'nullscape',
    title: 'Nullscape',
    description: 'Overhauls the End dimension with towering spires, abyssal voids, and varied floating islands.',
    author: 'Starmute',
    downloads: 12000000,
    icon_url: null,
    source: 'curseforge',
    project_type: 'datapack',
    project_url: 'https://www.curseforge.com/minecraft/customization/nullscape',
    distribution_disabled: false,
    categories: ['World Generation', 'Adventure'],
    client_side: 'optional',
    server_side: 'required'
  },
  {
    project_id: 'curseforge-700100',
    slug: 'worldedit',
    title: 'WorldEdit',
    description: 'In-game Minecraft map editor and fast voxel terraforming plugin.',
    author: 'sk89q',
    downloads: 75000000,
    icon_url: null,
    source: 'curseforge',
    project_type: 'plugin',
    project_url: 'https://www.curseforge.com/minecraft/bukkit-plugins/worldedit',
    distribution_disabled: false,
    categories: ['Management', 'Utility', 'World Generation'],
    client_side: 'unsupported',
    server_side: 'required'
  }
];

function previewModsKey(instanceName: string): string {
  return `redstone.preview.mods.${instanceName}`;
}

function previewTrackedModsKey(instanceName: string): string {
  return `redstone.preview.modrinth.${instanceName}`;
}

function loadPreviewMods(instanceName: string): InstalledMod[] {
  return JSON.parse(localStorage.getItem(previewModsKey(instanceName)) ?? '[]') as InstalledMod[];
}

function savePreviewMods(instanceName: string, mods: InstalledMod[]): void {
  localStorage.setItem(previewModsKey(instanceName), JSON.stringify(mods));
}

function loadPreviewTrackedMods(instanceName: string): TrackedMod[] {
  return JSON.parse(localStorage.getItem(previewTrackedModsKey(instanceName)) ?? '[]') as TrackedMod[];
}

function savePreviewTrackedMods(instanceName: string, mods: TrackedMod[]): void {
  localStorage.setItem(previewTrackedModsKey(instanceName), JSON.stringify(mods));
}

export async function getCurseForgeApiKey(): Promise<string | null> {
  return getNetworkSecret('curseforge_api_key');
}

export async function saveCurseForgeApiKey(key: string): Promise<void> {
  return saveNetworkSecret('curseforge_api_key', key);
}

export async function searchMods(
  query: string,
  minecraftVersion: string,
  loader: string,
  source: 'modrinth' | 'curseforge' = 'modrinth',
  category: string = 'all',
  projectType: ProjectType = 'mod',
  environment: EnvironmentFilter = 'all'
): Promise<ModSearchResponse> {
  let results: ModSearchResult[] = [];
  if (isTauri()) {
    const cmd = source === 'curseforge' ? 'search_curseforge_mods' : 'search_mods';
    results = await invoke<ModSearchResult[]>(cmd, {
      query: query.trim() || 'a',
      minecraftVersion,
      loader
    });
  } else {
    const pool = source === 'curseforge' ? previewCurseForgeCatalog : previewCatalog;
    results = [...pool];
  }

  if (category && category !== 'all') {
    const catLower = category.toLowerCase();
    results = results.filter((mod) =>
      mod.categories?.some((c) => c.toLowerCase().includes(catLower))
    );
  }

  if (projectType) {
    results = results.filter((mod) => (mod.project_type ?? 'mod') === projectType);
  }

  if (environment === 'client') {
    results = results.filter((mod) => mod.client_side !== 'unsupported');
  } else if (environment === 'server') {
    results = results.filter((mod) => mod.server_side !== 'unsupported');
  } else if (environment === 'both') {
    results = results.filter(
      (mod) => mod.client_side !== 'unsupported' && mod.server_side !== 'unsupported'
    );
  }

  const normalizedQuery = query.trim().toLowerCase();
  if (normalizedQuery) {
    results = results.filter((mod) =>
      `${mod.title} ${mod.description} ${mod.author}`.toLowerCase().includes(normalizedQuery)
    );
  }

  return { results, previewOnly: !isTauri() };
}

export async function installMod(
  instanceName: string,
  mod: ModSearchResult,
  minecraftVersion: string,
  loader: string
): Promise<ModInstallResponse> {
  if (mod.distribution_disabled) {
    throw new Error(`Mod distribution is disabled by author: ${mod.project_url ?? 'visit project page'}`);
  }

  if (isTauri()) {
    const isCurseForge = mod.source === 'curseforge' || mod.project_id.startsWith('curseforge-');
    const cmd = isCurseForge ? 'install_mod_from_curseforge' : 'install_mod_from_modrinth';
    const tracked = await invoke<TrackedMod>(cmd, {
      instanceName,
      projectId: mod.project_id,
      minecraftVersion,
      loader
    });
    return { fileName: tracked.file_name, versionId: tracked.version_id, previewOnly: false };
  }

  const fileName = `${mod.slug}.jar`;
  const installed = loadPreviewMods(instanceName);
  if (installed.some((entry) => entry.file_name === fileName)) {
    throw new Error(`Mod is already installed: ${fileName}`);
  }
  savePreviewMods(instanceName, [...installed, { file_name: fileName, enabled: true }]);
  const tracked: TrackedMod = { project_id: mod.project_id, file_name: fileName, version_id: 'preview-1' };
  savePreviewTrackedMods(instanceName, [...loadPreviewTrackedMods(instanceName), tracked]);
  return { fileName, versionId: tracked.version_id, previewOnly: true };
}

export async function checkModUpdates(
  instanceName: string,
  minecraftVersion: string,
  loader: string
): Promise<ModUpdate[]> {
  if (isTauri()) {
    return invoke<ModUpdate[]>('check_mod_updates', { instanceName, minecraftVersion, loader });
  }

  return loadPreviewTrackedMods(instanceName)
    .filter((mod) => mod.project_id === 'preview-sodium' && mod.version_id === 'preview-1')
    .map((mod) => ({
      project_id: mod.project_id,
      file_name: mod.file_name,
      current_version_id: mod.version_id,
      latest_version_id: 'preview-2',
      latest_version_number: '2.0.0',
      latest_file_name: mod.file_name.endsWith('.disabled') ? 'sodium-2.jar.disabled' : 'sodium-2.jar'
    }));
}

export async function applyModUpdate(
  instanceName: string,
  update: ModUpdate,
  minecraftVersion: string,
  loader: string,
  confirmed: boolean
): Promise<TrackedMod> {
  if (!confirmed) throw new Error('Applying a mod update requires confirmation.');
  if (isTauri()) {
    return invoke<TrackedMod>('apply_mod_update', {
      instanceName,
      update,
      minecraftVersion,
      loader,
      confirmed
    });
  }

  const tracked = loadPreviewTrackedMods(instanceName);
  const existing = tracked.find((mod) =>
    mod.project_id === update.project_id
    && mod.file_name === update.file_name
    && mod.version_id === update.current_version_id
  );
  if (!existing) throw new Error('Installed mod changed; refresh the update list.');
  const installed = loadPreviewMods(instanceName);
  const installedEntry = installed.find((mod) => mod.file_name === update.file_name);
  if (!installedEntry) throw new Error(`Mod not found: ${update.file_name}`);
  if (installed.some((mod) => mod.file_name === update.latest_file_name)) {
    throw new Error(`Mod is already installed: ${update.latest_file_name}`);
  }

  const updated: TrackedMod = {
    project_id: update.project_id,
    file_name: update.latest_file_name,
    version_id: update.latest_version_id
  };
  savePreviewMods(instanceName, [
    ...installed.filter((mod) => mod.file_name !== update.file_name),
    { file_name: update.latest_file_name, enabled: installedEntry.enabled }
  ]);
  savePreviewTrackedMods(instanceName, [...tracked.filter((mod) => mod !== existing), updated]);
  return updated;
}

export async function listInstalledMods(instanceName: string): Promise<InstalledModsResponse> {
  if (isTauri()) {
    const mods = await invoke<InstalledMod[]>('list_installed_mods', { instanceName });
    return { mods, previewOnly: false };
  }

  return { mods: loadPreviewMods(instanceName), previewOnly: true };
}

export async function setModEnabled(
  instanceName: string,
  fileName: string,
  enabled: boolean
): Promise<void> {
  if (isTauri()) {
    await invoke<string>('set_mod_enabled', { instanceName, fileName, enabled });
    return;
  }

  const mods = loadPreviewMods(instanceName);
  const index = mods.findIndex((entry) => entry.file_name === fileName);
  if (index < 0) throw new Error(`Mod not found: ${fileName}`);
  mods[index] = { ...mods[index], enabled };
  savePreviewMods(instanceName, mods);
  const tracked = loadPreviewTrackedMods(instanceName).map((mod) =>
    mod.file_name === fileName
      ? { ...mod, file_name: enabled ? fileName.replace(/\.disabled$/, '') : `${fileName}.disabled` }
      : mod
  );
  savePreviewTrackedMods(instanceName, tracked);
}

export async function removeMod(
  instanceName: string,
  fileName: string,
  confirmed: boolean
): Promise<void> {
  if (!confirmed) throw new Error('Removing a mod requires confirmation.');
  if (isTauri()) {
    await invoke<void>('remove_mod', { instanceName, fileName, confirmed });
    return;
  }

  const mods = loadPreviewMods(instanceName);
  if (!mods.some((entry) => entry.file_name === fileName)) {
    throw new Error(`Mod not found: ${fileName}`);
  }
  savePreviewMods(instanceName, mods.filter((entry) => entry.file_name !== fileName));
  savePreviewTrackedMods(
    instanceName,
    loadPreviewTrackedMods(instanceName).filter((entry) => entry.file_name !== fileName)
  );
}

export function createDebouncer(callback: () => void, delayMs: number) {
  let timeout: ReturnType<typeof setTimeout> | undefined;

  return {
    schedule() {
      if (timeout !== undefined) clearTimeout(timeout);
      timeout = setTimeout(() => {
        timeout = undefined;
        callback();
      }, delayMs);
    },
    cancel() {
      if (timeout !== undefined) clearTimeout(timeout);
      timeout = undefined;
    }
  };
}