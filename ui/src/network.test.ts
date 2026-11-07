import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  deleteNetworkSecret,
  getJoinInfo,
  getNetworkSecret,
  saveNetworkSecret,
  updateDnsRecords
} from './network';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn()
}));

describe('network adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key)
    });
  });

  afterEach(() => vi.unstubAllGlobals());

  it('computes LAN join info in preview mode by default', async () => {
    const info = await getJoinInfo('Survival');
    expect(info.is_lan).toBe(true);
    expect(info.port).toBe(25565);
    expect(info.address).toBe('192.168.1.100');
    expect(info.qr_payload).toBe('192.168.1.100:25565');
  });

  it('computes online join info in preview mode when public', async () => {
    storage.set(
      'redstone.settings',
      JSON.stringify({
        lan_only: false,
        dns_subdomain: 'mc',
        dns_domain: 'example.com'
      })
    );

    const info = await getJoinInfo('Survival');
    expect(info.is_lan).toBe(false);
    expect(info.address).toBe('mc.example.com');
    expect(info.qr_payload).toBe('mc.example.com');
  });

  it('computes playit tunnel join info in preview mode', async () => {
    storage.set(
      'redstone.settings',
      JSON.stringify({
        lan_only: false,
        dns_provider: 'playit',
        dns_subdomain: 'myserver'
      })
    );

    const info = await getJoinInfo('Survival');
    expect(info.is_lan).toBe(false);
    expect(info.address).toBe('myserver.gl.joinmc.link');
    expect(info.qr_payload).toBe('myserver.gl.joinmc.link');
  });

  it('saves, retrieves, and deletes network secrets in preview', async () => {
    expect(await getNetworkSecret('duckdns_token')).toBeNull();

    await saveNetworkSecret('duckdns_token', 'token-12345');
    expect(await getNetworkSecret('duckdns_token')).toBe('token-12345');

    await deleteNetworkSecret('duckdns_token');
    expect(await getNetworkSecret('duckdns_token')).toBeNull();
  });

  it('updates DNS records in preview when secret exists', async () => {
    await expect(
      updateDnsRecords({
        provider: 'duckdns',
        domain: 'mycoolmc',
        tokenKey: 'duckdns_token'
      })
    ).rejects.toThrow(/No credential found/);

    await saveNetworkSecret('duckdns_token', 'valid-token');
    const result = await updateDnsRecords({
      provider: 'duckdns',
      domain: 'mycoolmc',
      tokenKey: 'duckdns_token'
    });
    expect(result.toLowerCase()).toContain('duckdns');
  });

  it('delegates to native Tauri commands when in Tauri environment', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValueOnce({
      address: 'play.custom.net',
      port: 25565,
      is_lan: false,
      qr_payload: 'play.custom.net'
    } as never);

    const info = await getJoinInfo('Survival');
    expect(info.address).toBe('play.custom.net');
    expect(invoke).toHaveBeenCalledWith('get_join_info', { instanceName: 'Survival' });

    vi.mocked(invoke).mockResolvedValueOnce(undefined as never);
    await saveNetworkSecret('cloudflare_token', 'secret-cf');
    expect(invoke).toHaveBeenCalledWith('save_network_secret', {
      key: 'cloudflare_token',
      secret: 'secret-cf'
    });

    vi.mocked(invoke).mockResolvedValueOnce('secret-cf' as never);
    const token = await getNetworkSecret('cloudflare_token');
    expect(token).toBe('secret-cf');
    expect(invoke).toHaveBeenCalledWith('get_network_secret', { key: 'cloudflare_token' });

    vi.mocked(invoke).mockResolvedValueOnce(undefined as never);
    await deleteNetworkSecret('cloudflare_token');
    expect(invoke).toHaveBeenCalledWith('delete_network_secret', { key: 'cloudflare_token' });

    vi.mocked(invoke).mockResolvedValueOnce('Success' as never);
    const updateRes = await updateDnsRecords({
      provider: 'cloudflare',
      domain: 'play.custom.net',
      tokenKey: 'cloudflare_token',
      zoneId: 'zone1'
    });
    expect(updateRes).toBe('Success');
  });
});
