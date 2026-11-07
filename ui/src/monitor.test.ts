import { invoke, isTauri } from '@tauri-apps/api/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { formatUptime, readResourceMetrics } from './monitor';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));

describe('resource monitor adapter', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauri).mockReset().mockReturnValue(false);
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value)
    });
  });

  afterEach(() => vi.unstubAllGlobals());

  it('returns persisted preview metrics', async () => {
    await expect(readResourceMetrics('Survival')).resolves.toMatchObject({
      tps: 20,
      low_tps_alert: false,
      pid: 14820,
      uptime_seconds: 432,
      cpu_ticks: 12480,
      previewOnly: true
    });
  });

  it('formats uptime correctly', () => {
    expect(formatUptime(null)).toBe('Stopped');
    expect(formatUptime(undefined)).toBe('Stopped');
    expect(formatUptime(45)).toBe('00:45');
    expect(formatUptime(125)).toBe('02:05');
    expect(formatUptime(3665)).toBe('1h 01m 05s');
  });

  it('uses the native metrics command', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({
      cpu_percent: 42,
      memory_mb: 4096,
      heap_used_mb: 2048,
      heap_max_mb: 8192,
      tps: 18,
      low_tps_alert: false
    } as never);

    await expect(readResourceMetrics('Survival')).resolves.toMatchObject({
      cpu_percent: 42,
      heap_max_mb: 8192,
      previewOnly: false
    });
    expect(invoke).toHaveBeenCalledWith('read_resource_metrics', { instanceName: 'Survival' });
  });
});
