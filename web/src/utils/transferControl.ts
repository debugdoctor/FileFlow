import { fetchWithTimeout, waitWithSignal } from './http';

export type TransferRole = 'sender' | 'receiver';
export class PeerCancelledError extends Error {
  constructor() { super('对方已停止传输'); this.name = 'PeerCancelledError'; }
}

export const createSenderToken = () => Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, '0')).join('');

// Cancellation must outlive the local request AbortController.
export const cancelTransfer = async (id: string, role: TransferRole, credential: string) => {
  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      return await fetchWithTimeout(`/api/fileflow/${id}/cancel`, {
        method: 'POST', keepalive: true, headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(role === 'sender' ? { role, sender_token: credential } : { role, rid: credential }),
      }, 3000, async response => {
        if (response.status === 409) return; // Completion won the race; never undo it.
        const result = await response.json() as { success?: boolean };
        if (!response.ok || !result.success) throw new Error('无法通知对方停止传输');
      });
    } catch (error) {
      if (attempt === 2) throw error;
      await waitWithSignal(300 * (attempt + 1));
    }
  }
};

// One bounded request at a time; stop polling on completion, local cancellation or unmount.
export const watchTransfer = (id: string, role: TransferRole, onPeerCancelled: () => void, signal?: AbortSignal) => {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stop = () => {
    controller.abort();
    if (timer) clearTimeout(timer);
    signal?.removeEventListener('abort', stop);
  };
  if (signal?.aborted) { stop(); return stop; }
  signal?.addEventListener('abort', stop, { once: true });
  const poll = async () => {
    try {
      const state = await fetchWithTimeout(`/api/fileflow/${id}/status`, { signal: controller.signal }, 2500, async response => {
        if (!response.ok) return null;
        return await response.json() as { data?: { cancelled_by?: TransferRole; done?: boolean } };
      });
      if (controller.signal.aborted) return;
      if (state?.data?.cancelled_by) {
        stop();
        if (state.data.cancelled_by !== role) onPeerCancelled();
        return;
      }
      if (state?.data?.done) { stop(); return; }
    } catch { /* A brief network outage should not cancel the transfer itself. */ }
    if (!controller.signal.aborted) timer = setTimeout(poll, 700);
  };
  void poll();
  return stop;
};
