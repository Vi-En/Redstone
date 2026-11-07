import { invoke, isTauri } from '@tauri-apps/api/core';

export interface ResourceMetrics {
  cpu_percent: number;
  memory_mb: number;
  heap_used_mb: number;
  heap_max_mb: number;
  tps: number | null;
  low_tps_alert: boolean;
  pid?: number | null;
  uptime_seconds?: number | null;
  cpu_ticks?: number;
  previewOnly: boolean;
}

export function formatUptime(seconds?: number | null): string {
  if (seconds == null) return 'Stopped';
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  if (hrs > 0) {
    return `${hrs}h ${mins.toString().padStart(2, '0')}m ${secs.toString().padStart(2, '0')}s`;
  }
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

export interface MetricHistoryPoint {
  time: number;
  tps: number;
  cpu: number;
  ram: number;
  heap: number;
  heapMax: number;
}

const previewKey = (instanceName: string) => `redstone.preview.metrics.${instanceName}`;

export async function readResourceMetrics(instanceName: string): Promise<ResourceMetrics> {
  if (isTauri()) {
    const metrics = await invoke<Omit<ResourceMetrics, 'previewOnly'>>('read_resource_metrics', {
      instanceName
    });
    return { ...metrics, previewOnly: false };
  }

  const stored = localStorage.getItem(previewKey(instanceName));
  if (!stored) {
    const initial: ResourceMetrics = {
      cpu_percent: 12.4,
      memory_mb: 3072,
      heap_used_mb: 1180,
      heap_max_mb: 4096,
      tps: 20,
      low_tps_alert: false,
      pid: 14820,
      uptime_seconds: 432,
      cpu_ticks: 12480,
      previewOnly: true
    };
    localStorage.setItem(previewKey(instanceName), JSON.stringify(initial));
    return initial;
  }

  const previous = JSON.parse(stored) as ResourceMetrics;
  // Dynamic slight variations for live graphing in preview mode
  const cpuJitter = (Math.random() * 4 - 2);
  const nextCpu = Math.max(4.0, Math.min(58.0, Number((previous.cpu_percent + cpuJitter).toFixed(1))));
  const ramJitter = Math.floor(Math.random() * 16 - 8);
  const nextRam = Math.max(2048, Math.min(3800, previous.memory_mb + ramJitter));
  // Heap simulation with subtle garbage collection cycling
  let nextHeap = previous.heap_used_mb + Math.floor(Math.random() * 45 - 12);
  if (nextHeap > 2600) {
    nextHeap = 950 + Math.floor(Math.random() * 100); // GC triggered!
  }
  const tpsJitter = (Math.random() * 0.2 - 0.1);
  const nextTps = Math.min(20.0, Math.max(18.8, Number(((previous.tps ?? 20.0) + tpsJitter).toFixed(2))));
  const nextUptime = (previous.uptime_seconds ?? 432) + 1;
  const nextTicks = (previous.cpu_ticks ?? 12480) + 12;

  const updated: ResourceMetrics = {
    ...previous,
    cpu_percent: nextCpu,
    memory_mb: nextRam,
    heap_used_mb: nextHeap,
    tps: nextTps,
    uptime_seconds: nextUptime,
    cpu_ticks: nextTicks,
    previewOnly: true
  };
  localStorage.setItem(previewKey(instanceName), JSON.stringify(updated));
  return updated;
}
