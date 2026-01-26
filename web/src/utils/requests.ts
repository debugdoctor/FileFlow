import message from "ant-design-vue/es/message";
import type { Ref } from "vue";

const MAX_RETRIES = 4;
const BASE_DELAY_MS = 400;
const MAX_DELAY_MS = 2500;
const DEFAULT_TIMEOUT_MS = 10000;
const DEFAULT_RETRIES = 2;

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

const isRetryableStatus = (status: number) =>
  status === 408 || status === 425 || status === 429 || (status >= 500 && status <= 599);

const fetchWithTimeout = async (input: RequestInfo, init: RequestInit, timeoutMs: number): Promise<Response> => {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(input, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timeoutId);
  }
};

const parseJsonSafely = async <T>(response: Response): Promise<T | null> => {
  try {
    return (await response.json()) as T;
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

export const fetchWithRetry = async (
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
): Promise<Response> => {
  let attempt = 0;
  let lastError: unknown;

  while (attempt <= retries) {
    try {
      const response = await fetchWithTimeout(input, init, timeoutMs);
      if (response.ok || !retryOnStatus(response.status)) {
        return response;
      }
      lastError = new Error(`Request failed with status ${response.status}`);
    } catch (error) {
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
    await sleep(backoff + jitter);
  }

  throw lastError instanceof Error ? lastError : new Error("Request failed after retries");
};

export const fetchJsonWithRetry = async <T>(
  input: RequestInfo,
  init: RequestInit = {},
  options?: RetryOptions,
): Promise<{ data: T | null; response: Response }> => {
  const response = await fetchWithRetry(input, init, options);
  const data = await parseJsonSafely<T>(response);
  return { data, response };
};

export const uploadFile = async (formData: FormData, accessId: string | null, i: number, seq_len: number): Promise<number> => {
  let attempt = 0;

  while (attempt <= MAX_RETRIES) {
    try {
      const response = await fetchWithTimeout(
        `/api/fileflow/${accessId}/upload`,
        { method: "post", body: formData },
        18000
      );

      let body: any = null;
      let messageFromServer: string | undefined;
      try {
        body = await response.json();
        messageFromServer = body?.message;
      } catch {
        if (!response.ok) {
          throw new Error(`Upload failed for chunk ${i + 1} with status ${response.status}`);
        }
      }

      if (!response.ok || (body && body.code !== 200)) {
        const err = new Error(messageFromServer || `Upload failed for chunk ${i + 1} with status ${response.status}`);
        (err as any).status = response.status;
        throw err;
      }

      return seq_len;
    } catch (error: unknown) {
      attempt += 1;
      const status = (error as any)?.status as number | undefined;
      const retryable = status ? isRetryableStatus(status) : true;
      const errMsg = error instanceof Error ? error.message : "Upload failed.";

      if (attempt > MAX_RETRIES || !retryable) {
        message.error(errMsg);
        throw new Error(`Upload failed for chunk ${i + 1}: ${errMsg}`);
      }

      const backoff = Math.min(BASE_DELAY_MS * Math.pow(2, attempt - 1), MAX_DELAY_MS);
      const jitter = Math.floor(Math.random() * 150);
      await sleep(backoff + jitter);
    }
  }

  throw new Error(`Upload failed for chunk ${i + 1} after ${MAX_RETRIES} retries`);
};

export const downloadFile = async (fileId: string, start: number, fileName: Ref<string>): Promise<[RegExpMatchArray, Response]> => {
  let attempt = 0;

  while (attempt <= MAX_RETRIES) {
    try {
      const response = await fetchWithTimeout(
        `/api/fileflow/${fileId}/file?rid=${localStorage.getItem("rid")}&start=${start}`,
        { method: "get" },
        18000
      );

      if (!response.ok) {
        const err = new Error(`Download failed with status ${response.status}`);
        (err as any).status = response.status;
        throw err;
      }

      const contentRange = response.headers.get("Content-Range");
      const contentName = response.headers.get("Content-Name");
      if (contentName) {
        fileName.value = contentName;
      }

      if (contentRange) {
        const rangeMatch = contentRange.match(/bytes\s+(\d+)-(\d+)\/(\d+)/);
        if (rangeMatch) {
          return [rangeMatch, response];
        }
      }

      throw new Error("Invalid Content-Range");
    } catch (error: unknown) {
      attempt += 1;
      const status = (error as any)?.status as number | undefined;
      const retryable = status ? isRetryableStatus(status) : true;

      if (attempt > MAX_RETRIES || !retryable) {
        throw new Error(`Download failed for chunk ${start}: ${(error as Error).message}`);
      }

      const backoff = Math.min(BASE_DELAY_MS * Math.pow(2, attempt - 1), MAX_DELAY_MS);
      const jitter = Math.floor(Math.random() * 150);
      await sleep(backoff + jitter);
    }
  }

  throw new Error(`Download failed for chunk ${start} after ${MAX_RETRIES} retries`);
};
