import { invoke, isTauri } from '@tauri-apps/api/core';

export type JvmPreset = 'Custom' | 'Performance';

export interface JvmSettings {
  ramMb: number;
  preset: JvmPreset;
  custom: string;
}

const previewKey = (instanceName: string) => `redstone.preview.jvm.${instanceName}`;
const defaultSettings: JvmSettings = { ramMb: 4096, preset: 'Custom', custom: '' };

export async function readJvmSettings(instanceName: string): Promise<JvmSettings> {
  if (isTauri()) return invoke<JvmSettings>('read_jvm_settings', { instanceName });
  const stored = localStorage.getItem(previewKey(instanceName));
  return stored ? { ...defaultSettings, ...JSON.parse(stored) as Partial<JvmSettings> } : { ...defaultSettings };
}

export async function writeJvmSettings(instanceName: string, settings: JvmSettings): Promise<void> {
  if (isTauri()) {
    await invoke<void>('write_jvm_settings', { instanceName, settings });
    return;
  }
  localStorage.setItem(previewKey(instanceName), JSON.stringify(settings));
}

export function systemMemoryMb(): number {
  const deviceMemory = (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 8;
  return Math.max(1024, Math.round(deviceMemory * 1024));
}
