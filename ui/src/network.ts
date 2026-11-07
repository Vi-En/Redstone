import { invoke, isTauri } from '@tauri-apps/api/core';

export interface JoinInfo {
  address: string;
  port: number;
  is_lan: boolean;
  qr_payload: string;
}

export interface UpdateDnsParams {
  provider: string;
  domain: string;
  tokenKey: string;
  zoneId?: string;
  ip?: string;
}

const previewSecretsPrefix = 'redstone.preview.secret.';

export async function getJoinInfo(instanceName: string): Promise<JoinInfo> {
  if (isTauri()) {
    return invoke<JoinInfo>('get_join_info', { instanceName });
  }

  // Preview fallback
  const rawSettings = localStorage.getItem('redstone.settings');
  let lanOnly = true;
  let subdomain = 'play';
  let domain = 'example.com';
  let provider = 'manual';
  if (rawSettings) {
    try {
      const parsed = JSON.parse(rawSettings);
      lanOnly = parsed.lan_only ?? true;
      subdomain = parsed.dns_subdomain ?? 'play';
      domain = parsed.dns_domain ?? 'example.com';
      provider = parsed.dns_provider ?? 'manual';
    } catch {
      // ignore
    }
  }

  if (lanOnly) {
    const address = '192.168.1.100';
    const port = 25565;
    return {
      address,
      port,
      is_lan: true,
      qr_payload: `${address}:${port}`
    };
  }

  const fullHostname = provider === 'playit' ? `${subdomain}.gl.joinmc.link` : `${subdomain}.${domain}`;
  const port = 25565;
  return {
    address: fullHostname,
    port,
    is_lan: false,
    qr_payload: port === 25565 ? fullHostname : `${fullHostname}:${port}`
  };
}

export async function saveNetworkSecret(key: string, secret: string): Promise<void> {
  if (isTauri()) {
    return invoke('save_network_secret', { key, secret });
  }
  localStorage.setItem(`${previewSecretsPrefix}${key}`, secret);
}

export async function getNetworkSecret(key: string): Promise<string | null> {
  if (isTauri()) {
    return invoke<string | null>('get_network_secret', { key });
  }
  return localStorage.getItem(`${previewSecretsPrefix}${key}`);
}

export async function deleteNetworkSecret(key: string): Promise<void> {
  if (isTauri()) {
    return invoke('delete_network_secret', { key });
  }
  localStorage.removeItem(`${previewSecretsPrefix}${key}`);
}

export async function updateDnsRecords(params: UpdateDnsParams): Promise<string> {
  if (isTauri()) {
    return invoke<string>('update_dns_records', {
      provider: params.provider,
      domain: params.domain,
      tokenKey: params.tokenKey,
      zoneId: params.zoneId ?? null,
      ip: params.ip ?? null
    });
  }

  const token = localStorage.getItem(`${previewSecretsPrefix}${params.tokenKey}`);
  if (!token || !token.trim()) {
    throw new Error(`No credential found in keychain for '${params.tokenKey}'`);
  }

  return `Preview: ${params.provider} record for '${params.domain}' updated successfully`;
}
