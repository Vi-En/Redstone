import { invoke } from '@tauri-apps/api/core';

export interface PairingTicket {
  endpoint_id: string;
  ticket_code: string;
  qr_payload: string;
}

export interface RemoteClient {
  endpoint_id: string;
  paired: boolean;
}

const PREVIEW_KEY_DEVICES = 'redstone_paired_devices';
const PREVIEW_KEY_TICKET = 'redstone_active_ticket';

function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function getRemotePairingTicket(): Promise<PairingTicket> {
  if (isTauriEnvironment()) {
    return invoke<PairingTicket>('get_remote_pairing_ticket');
  }

  // Preview / browser mode fallback
  const endpointId = 'redstone-preview-host';
  const ticketCode = 'a1b2c3';
  const ticket: PairingTicket = {
    endpoint_id: endpointId,
    ticket_code: ticketCode,
    qr_payload: `redstone://pair/${endpointId}/${ticketCode}`
  };
  localStorage.setItem(PREVIEW_KEY_TICKET, ticketCode);
  return ticket;
}

export async function listPairedDevices(): Promise<string[]> {
  if (isTauriEnvironment()) {
    return invoke<string[]>('list_paired_devices');
  }

  const raw = localStorage.getItem(PREVIEW_KEY_DEVICES);
  if (!raw) return ['Pixel 8 (preview)'];
  try {
    return JSON.parse(raw) as string[];
  } catch {
    return [];
  }
}

export async function revokePairedDevice(endpointId: string): Promise<boolean> {
  if (isTauriEnvironment()) {
    return invoke<boolean>('revoke_paired_device', { endpointId });
  }

  const devices = await listPairedDevices();
  const next = devices.filter((d) => d !== endpointId);
  localStorage.setItem(PREVIEW_KEY_DEVICES, JSON.stringify(next));
  return true;
}

export async function pairRemoteDevice(clientEndpointId: string, ticketCode: string): Promise<RemoteClient> {
  if (isTauriEnvironment()) {
    return invoke<RemoteClient>('pair_remote_device', { clientEndpointId, ticketCode });
  }

  const activeTicket = localStorage.getItem(PREVIEW_KEY_TICKET) ?? 'a1b2c3';
  if (ticketCode.trim() !== activeTicket) {
    throw new Error('invalid or expired pairing ticket');
  }

  const devices = await listPairedDevices();
  if (!devices.includes(clientEndpointId)) {
    devices.push(clientEndpointId);
    localStorage.setItem(PREVIEW_KEY_DEVICES, JSON.stringify(devices));
  }

  return {
    endpoint_id: clientEndpointId,
    paired: true
  };
}
