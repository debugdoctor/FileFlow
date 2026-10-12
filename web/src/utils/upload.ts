import { fetchWithTimeout, waitWithSignal } from './http';

type Admission =
  | { status: 'ready'; token: string; expires_ms: number }
  | { status: 'complete' }
  | { status: 'waiting'; retry_after_ms: number };

export const uploadFile = async (
  createFormData: () => FormData, accessId: string | null, index: number, length: number, start: number, signal?: AbortSignal,
): Promise<number> => {
  const deadline = Date.now() + 120000;
  let failures = 0;
  while (Date.now() < deadline) {
    signal?.throwIfAborted();
    try {
      const admission = await fetchWithTimeout(
        `/api/fileflow/${accessId}/admit`,
        { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ start, length }), signal },
        6000,
        async response => {
          const payload = await response.json() as { success?: boolean; message?: string; data?: Admission };
          if (!response.ok || !payload.success || !payload.data) {
            const error = new Error(payload.message || `Admission failed: ${response.status}`);
            Object.assign(error, { status: response.status });
            throw error;
          }
          return payload.data;
        },
      );
      if (admission.status === 'complete') return length;
      if (admission.status === 'waiting') {
        await waitWithSignal(Math.max(100, Math.min(admission.retry_after_ms, 2000)) + Math.floor(Math.random() * 100), signal);
        continue;
      }
      if (admission.status !== 'ready' || !admission.token) throw new Error('Invalid upload admission');
      signal?.throwIfAborted();
      const response = await fetchWithTimeout(
        `/api/fileflow/${accessId}/upload`,
        { method: 'POST', headers: { 'X-Upload-Permit': admission.token }, body: createFormData(), signal },
        18000,
        async response => {
          const payload = await response.json() as { code?: number; message?: string };
          return { status: response.status, ok: response.ok, payload };
        },
      );
      if (response.status === 409) continue; // Expired or already consumed; re-check admission.
      if (!response.ok || response.payload.code !== 200) {
        const error = new Error(response.payload.message || `Upload failed: ${response.status}`);
        Object.assign(error, { status: response.status });
        throw error;
      }
      return length;
    } catch (error) {
      if (signal?.aborted) throw error;
      failures++;
      const status = (error as { status?: number })?.status;
      const retryable = status === undefined || status === 408 || status === 425 || status === 429 || status >= 500;
      if (!retryable || failures > 4) {
        throw new Error(`Upload failed for chunk ${index + 1}: ${error instanceof Error ? error.message : 'Unknown error'}`);
      }
      await waitWithSignal(Math.min(400 * 2 ** (failures - 1), 2500) + Math.floor(Math.random() * 150), signal);
    }
  }
  throw new Error(`等待分块 ${index + 1} 的传输资源超时，请稍后重试`);
};
