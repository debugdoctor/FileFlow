// Await both the request and its response processing before reusing a worker.
const processWithConcurrencyLimit = async <T>(
  tasks: Array<() => Promise<T>>,
  limit: number,
  onProgress?: (result: T) => void | Promise<void>,
): Promise<void> => {
  if (!Number.isInteger(limit) || limit < 1) {
    throw new Error('Concurrency limit must be a positive integer');
  }
  let currentIndex = 0;
  let failed = false;
  let failure: unknown;
  const worker = async () => {
    while (!failed && currentIndex < tasks.length) {
      const task = tasks[currentIndex++];
      try {
        const result = await task();
        await onProgress?.(result);
      } catch (error) {
        if (!failed) failure = error;
        failed = true;
      }
    }
  };
  // Drain in-flight requests before callers reset transfer state.
  await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, worker));
  if (failed) throw failure;
};

export const processUploadWithConcurrencyLimit = (
  tasks: Array<() => Promise<number>>,
  limit: number,
  onProgress?: (length: number) => void | Promise<void>,
): Promise<void> => processWithConcurrencyLimit(tasks, limit, onProgress);

export const processDownloadWithConcurrencyLimit = (
  tasks: Array<() => Promise<[RegExpMatchArray, Response]>>,
  limit: number,
  onProgress?: (range: RegExpMatchArray, response: Response) => void | Promise<void>,
): Promise<void> => processWithConcurrencyLimit(
  tasks,
  limit,
  onProgress ? ([range, response]) => onProgress(range, response) : undefined,
);
