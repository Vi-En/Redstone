import { invoke, isTauri } from '@tauri-apps/api/core';

export interface PlayerEntry {
  name: string;
  uuid: string;
}

export interface PlayerLists {
  whitelist: PlayerEntry[];
  banlist: PlayerEntry[];
}

const previewPlayersKey = (instanceName: string) => `redstone.preview.players.${instanceName}`;

function defaultPreviewPlayers(): PlayerLists {
  return {
    whitelist: [
      { name: 'Alex', uuid: '069a79f4-44e9-4726-a5be-fca90e38aaf5' },
      { name: 'Steve', uuid: '853c80ef-3c37-49fd-aa49-938b674adae6' }
    ],
    banlist: [
      { name: 'Griefer123', uuid: '11111111-2222-3333-4444-555555555555' }
    ]
  };
}

function getPreviewPlayers(instanceName: string): PlayerLists {
  const stored = localStorage.getItem(previewPlayersKey(instanceName));
  if (stored) {
    try {
      return JSON.parse(stored) as PlayerLists;
    } catch {
      // fallback
    }
  }
  const defaults = defaultPreviewPlayers();
  localStorage.setItem(previewPlayersKey(instanceName), JSON.stringify(defaults));
  return defaults;
}

function setPreviewPlayers(instanceName: string, data: PlayerLists): void {
  localStorage.setItem(previewPlayersKey(instanceName), JSON.stringify(data));
}

export function generateOfflineUuid(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash << 5) - hash + name.charCodeAt(i);
    hash |= 0;
  }
  const hex = Math.abs(hash).toString(16).padStart(8, '0');
  return `${hex.slice(0, 8)}-0000-4000-8000-${hex.padEnd(12, '0').slice(0, 12)}`;
}

export async function readPlayerLists(instanceName: string): Promise<PlayerLists> {
  if (isTauri()) {
    return invoke<PlayerLists>('read_player_lists', { instanceName });
  }
  return getPreviewPlayers(instanceName);
}

export async function writeWhitelist(instanceName: string, entries: PlayerEntry[]): Promise<void> {
  if (isTauri()) {
    await invoke<void>('write_whitelist', { instanceName, entries });
    return;
  }
  const players = getPreviewPlayers(instanceName);
  players.whitelist = entries;
  setPreviewPlayers(instanceName, players);
}

export async function writeBanlist(instanceName: string, entries: PlayerEntry[]): Promise<void> {
  if (isTauri()) {
    await invoke<void>('write_banlist', { instanceName, entries });
    return;
  }
  const players = getPreviewPlayers(instanceName);
  players.banlist = entries;
  setPreviewPlayers(instanceName, players);
}
