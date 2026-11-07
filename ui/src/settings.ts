import { invoke, isTauri } from '@tauri-apps/api/core';

export interface Settings {
  onboarding_complete: boolean;
  lan_only: boolean;
  instance_name: string | null;
  dns_provider: string | null;
  dns_subdomain: string | null;
  dns_domain: string | null;
}

export const defaultSettings: Settings = {
  onboarding_complete: false,
  lan_only: true,
  instance_name: null,
  dns_provider: null,
  dns_subdomain: null,
  dns_domain: null
};

const previewStorageKey = 'redstone.settings';

function isValidDnsName(name: string | null, singleLabel: boolean): boolean {
  if (name === null) return true;
  const labels = name.split('.');
  return name.length > 0
    && name.length <= 253
    && (!singleLabel || labels.length === 1)
    && labels.every((label) => label.length > 0
      && label.length <= 63
      && !label.startsWith('-')
      && !label.endsWith('-')
      && /^[A-Za-z0-9-]+$/.test(label));
}

function validateSettings(settings: Settings): void {
  if (!isValidDnsName(settings.dns_subdomain, true)) {
    throw new Error('DNS subdomain must be a single valid hostname label.');
  }
  if (!isValidDnsName(settings.dns_domain, false)) {
    throw new Error('DNS domain contains an invalid hostname label.');
  }
}

export async function loadSettings(): Promise<Settings> {
  if (isTauri()) {
    return invoke<Settings>('load_settings');
  }

  const storedSettings = localStorage.getItem(previewStorageKey);
  return storedSettings
    ? { ...defaultSettings, ...JSON.parse(storedSettings) as Partial<Settings> }
    : { ...defaultSettings };
}

export async function saveSettings(settings: Settings): Promise<void> {
  validateSettings(settings);

  if (isTauri()) {
    await invoke<void>('save_settings', { settings });
    return;
  }

  localStorage.setItem(previewStorageKey, JSON.stringify(settings));
}