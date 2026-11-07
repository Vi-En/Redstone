import { invoke, isTauri } from '@tauri-apps/api/core';

export type WorldProperties = Record<string, string>;

const previewKey = (instanceName: string) => `redstone.preview.properties.${instanceName}`;

const defaultProperties: WorldProperties = {
  'level-name': 'world',
  'gamemode': 'survival',
  difficulty: 'easy',
  'level-seed': '',
  'allow-cheats': 'false',
  'generate-structures': 'true',
  'online-mode': 'true'
};

export async function readWorldProperties(instanceName: string): Promise<WorldProperties> {
  if (isTauri()) return invoke<WorldProperties>('read_server_properties', { instanceName });
  const stored = localStorage.getItem(previewKey(instanceName));
  return stored ? { ...defaultProperties, ...JSON.parse(stored) as WorldProperties } : { ...defaultProperties };
}

export async function writeWorldProperties(
  instanceName: string,
  properties: WorldProperties
): Promise<void> {
  if (isTauri()) {
    await invoke<void>('write_server_properties', { instanceName, properties });
    return;
  }
  localStorage.setItem(previewKey(instanceName), JSON.stringify(properties));
}
