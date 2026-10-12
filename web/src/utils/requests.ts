import type { Ref } from "vue";
import { fetchWithTimeout, waitWithSignal } from './http';
export { uploadFile } from './upload';

const MAX_RETRIES = 4;
const BASE_DELAY_MS = 400;
const MAX_DELAY_MS = 2500;
const DEFAULT_TIMEOUT_MS = 10000;
const DEFAULT_RETRIES = 2;

const isRetryableStatus = (status: number) =>
  status === 408 || status === 425 || status === 429 || (status >= 500 && status <= 599);

const parseJsonSafely = async <T>(response: Response): Promise<T | null> => {
  const text = await response.text();
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
};

const shouldRetry = (status?: number) => (status === undefined ? true : isRetryableStatus(status));

interface RetryOptions {
  retries?: number;
  timeoutMs?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  jitterMs?: number;
  retryOnStatus?: (status?: number) => boolean;
}

const fetchAndConsumeWithRetry = async <T>(
  input: RequestInfo,
  init: RequestInit = {},
  {
    retries = DEFAULT_RETRIES,
    timeoutMs = DEFAULT_TIMEOUT_MS,
    baseDelayMs = BASE_DELAY_MS,
    maxDelayMs = MAX_DELAY_MS,
    jitterMs = 150,
    retryOnStatus = shouldRetry,
  }: RetryOptions = {},
  consume: (response: Response) => Promise<T>,
): Promise<T> => {
  let attempt = 0;
  let lastError: unknown;

  while (attempt <= retries) {
    try {
      return await fetchWithTimeout(input, init, timeoutMs, async response => {
        if (response.ok || !retryOnStatus(response.status)) return await consume(response);
        await response.body?.cancel();
        throw new Error(`Request failed with status ${response.status}`);
      });
    } catch (error) {
      if (init.signal?.aborted) throw error;
      lastError = error;
      // AbortError usually means timeout; keep retrying unless we exhausted attempts.
      if (error instanceof DOMException && error.name === "AbortError" && attempt >= retries) {
        throw error;
      }
    }

    attempt += 1;
    if (attempt > retries) break;

    const backoff = Math.min(baseDelayMs * Math.pow(2, attempt - 1), maxDelayMs);
    const jitter = Math.floor(Math.random() * jitterMs);
    await waitWithSignal(backoff + jitter, init.signal || undefined);
  }

  throw lastError instanceof Error ? lastError : new Error("Request failed after retries");
};

export const fetchWithRetry = (
  input: RequestInfo, init: RequestInit = {}, options: RetryOptions = {},
): Promise<Response> => fetchAndConsumeWithRetry(input, init, options, async response => response);

export const fetchJsonWithRetry = async <T>(
  input: RequestInfo,
  init: RequestInit = {},
  options?: RetryOptions,
): Promise<{ data: T | null; response: Response }> => {
  return fetchAndConsumeWithRetry(input, init, options, async response => ({
    data: await parseJsonSafely<T>(response), response,
  }));
};

export const downloadFile = async (
  fileId: string, start: number, fileName: Ref<string>, total: number, length: number, signal?: AbortSignal,
): Promise<[RegExpMatchArray, ArrayBuffer]> => {
  let attempt = 0;

  while (attempt <= MAX_RETRIES) {
    try {
      return await fetchWithTimeout(
        `/api/fileflow/${fileId}/file?rid=${localStorage.getItem("rid")}&start=${start}`,
        { method: "get", signal },
        18000,
        async (response): Promise<[RegExpMatchArray, ArrayBuffer]> => {
          if (response.status !== 206) {
            await response.body?.cancel();
            const err = new Error(`Download failed with status ${response.status}`);
            (err as any).status = response.status;
            throw err;
          }

          const contentRange = response.headers.get("Content-Range");
          const contentName = response.headers.get("Content-Name");
          if (contentName) {
            try {
              fileName.value = decodeURIComponent(contentName);
            } catch {
              fileName.value = contentName;
            }
          }

          if (contentRange) {
            const rangeMatch = contentRange.match(/^bytes\s+(\d+)-(\d+)\/(\d+)$/);
            if (rangeMatch && Number(rangeMatch[1]) === start && Number(rangeMatch[2]) === start + length - 1
                && Number(rangeMatch[3]) === total) {
              const buffer = await response.arrayBuffer();
              if (buffer.byteLength !== length) throw new Error('Incomplete file block');
              return [rangeMatch, buffer];
            }
          }

          await response.body?.cancel();
          throw new Error("Invalid Content-Range");
        },
      );
    } catch (error: unknown) {
      if (signal?.aborted) throw error;
      attempt += 1;
      const status = (error as any)?.status as number | undefined;
      const retryable = status ? isRetryableStatus(status) : true;

      if (attempt > MAX_RETRIES || !retryable) {
        throw new Error(`Download failed for chunk ${start}: ${(error as Error).message}`);
      }

      const backoff = Math.min(BASE_DELAY_MS * Math.pow(2, attempt - 1), MAX_DELAY_MS);
      const jitter = Math.floor(Math.random() * 150);
      await waitWithSignal(backoff + jitter, signal);
    }
  }

  throw new Error(`Download failed for chunk ${start} after ${MAX_RETRIES} retries`);
};

export const acknowledgeBlock = async (fileId: string, start: number, signal?: AbortSignal): Promise<void> => {
  const { data, response } = await fetchJsonWithRetry<{ success?: boolean }>(
    `/api/fileflow/${fileId}/ack`,
    { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ rid: localStorage.getItem('rid'), start }), signal },
    { timeoutMs: 6000, retries: 4 },
  );
  if (!response.ok || !data?.success) throw new Error('无法确认文件分块，请重试');
};

export const getTransferConfig = async (signal?: AbortSignal): Promise<{ chunk_size: number; max_file_size: number }> => {
  const { data, response } = await fetchJsonWithRetry<{
    success?: boolean; data?: { chunk_size: number; max_file_size: number };
  }>('/api/fileflow/config', { signal });
  if (!response.ok || !data?.success || !data.data
      || !Number.isSafeInteger(data.data.chunk_size) || data.data.chunk_size <= 0
      || !Number.isSafeInteger(data.data.max_file_size) || data.data.max_file_size <= 0) {
    throw new Error('无法获取文件传输配置');
  }
  return data.data;
};
