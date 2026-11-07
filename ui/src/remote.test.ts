import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  getRemotePairingTicket,
  listPairedDevices,
  pairRemoteDevice,
  revokePairedDevice
} from './remote';

describe('remote client module', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
      removeItem: (key: string) => storage.delete(key),
      clear: () => storage.clear()
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('generates pairing ticket with valid qr payload', async () => {
    const ticket = await getRemotePairingTicket();
    expect(ticket.endpoint_id).toBe('redstone-preview-host');
    expect(ticket.ticket_code).toBe('a1b2c3');
    expect(ticket.qr_payload).toBe('redstone://pair/redstone-preview-host/a1b2c3');
  });

  it('lists paired devices and pairs a new device with ticket', async () => {
    await getRemotePairingTicket();
    const client = await pairRemoteDevice('android-device-42', 'a1b2c3');
    expect(client.endpoint_id).toBe('android-device-42');
    expect(client.paired).toBe(true);

    const devices = await listPairedDevices();
    expect(devices).toContain('android-device-42');
  });

  it('rejects pairing with invalid ticket code', async () => {
    await getRemotePairingTicket();
    await expect(pairRemoteDevice('android-device-42', 'wrong-code')).rejects.toThrow(
      'invalid or expired pairing ticket'
    );
  });

  it('revokes a paired device', async () => {
    await getRemotePairingTicket();
    await pairRemoteDevice('android-device-42', 'a1b2c3');
    let devices = await listPairedDevices();
    expect(devices).toContain('android-device-42');

    const revoked = await revokePairedDevice('android-device-42');
    expect(revoked).toBe(true);

    devices = await listPairedDevices();
    expect(devices).not.toContain('android-device-42');
  });
});
