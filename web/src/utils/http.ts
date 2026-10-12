// Keep the timeout active until the response body has been consumed.
export const fetchWithTimeout = async <T = Response>(
  input: RequestInfo,
  init: RequestInit,
  timeoutMs: number,
  consume: (response: Response) => Promise<T> = async response => response as unknown as T,
): Promise<T> => {
  const controller = new AbortController();
  const abort = () => controller.abort(init.signal?.reason);
  if (init.signal?.aborted) abort();
  else init.signal?.addEventListener('abort', abort, { once: true });
  const timeoutId = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(input, { ...init, signal: controller.signal, cache: 'no-store' });
    return await consume(response);
  } finally {
    clearTimeout(timeoutId);
    init.signal?.removeEventListener('abort', abort);
  }
};
