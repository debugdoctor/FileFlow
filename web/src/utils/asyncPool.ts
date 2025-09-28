// 通用的并发控制函数
const processWithConcurrencyLimit = async <T>(
  tasks: Array<() => Promise<T>>,
  limit: number,
  onProgress?: (result: T) => Promise<void>
): Promise<void> => {
  return new Promise(async (resolve, reject) => {
    let activePromises = 0;
    let currentIndex = 0;
    let completed = 0;
    let failed = 0;
    const total = tasks.length;
    const progressPromises: Promise<void>[] = [];

    const processNext = async () => {
      // If we've processed all tasks, check if we're done
      if (currentIndex >= total) {
        if (completed === total) {
          try {
            await Promise.all(progressPromises);
          } catch (progressError) {
            console.error("Progress callback failed:", progressError);
          }
          resolve();
        }
        return;
      }

      // While we have capacity and tasks to process
      while (activePromises < limit && currentIndex < total) {
        const taskIndex = currentIndex;
        const task = tasks[currentIndex];
        currentIndex++;
        activePromises++;

        // Execute the task
        task()
          .then(async (result) => {
            activePromises--;
            completed++;

            // Report progress if callback is provided
            if (onProgress) {
              progressPromises.push(onProgress(result));
            }

            // Check if all tasks are completed
            if (completed + failed === total) {
              try {
                await Promise.all(progressPromises);
              } catch (progressError) {
                console.error("Progress callback failed:", progressError);
              }
              if (failed > 0) {
                // Some tasks failed, reject the operation
                reject(new Error(`Some tasks failed. Total failed: ${failed}`));
              } else {
                resolve();
              }
            } else {
              // Process next available task
              await processNext();
            }
          })
          .catch(async (error) => {
            // Log the error and reject the entire operation
            console.error("Task failed:", error);
            activePromises--;
            failed++;
            completed++;

            // Check if all tasks are completed
            if (completed + failed === total) {
              try {
                await Promise.all(progressPromises);
              } catch (progressError) {
                console.error("Progress callback failed:", progressError);
              }
              if (failed > 0) {
                // Some tasks failed, reject the operation
                reject(new Error(`Some tasks failed. Total failed: ${failed}`));
              } else {
                resolve();
              }
            } else {
              // Process next available task
              await processNext();
            }
          });
      }
    };

    // Start processing
    processNext();
  });
};

export const processUploadWithConcurrencyLimit = async (
  tasks: Array<() => Promise<any>>,
  limit: number,
  onProgress?: (seq_len: number) => void
): Promise<void> => {
  return processWithConcurrencyLimit(
    tasks,
    limit,
    onProgress
      ? async (seq_len: number) => onProgress(seq_len)
      : undefined
  );
};

export const processDownloadWithConcurrencyLimit = async (
  tasks: Array<() => Promise<any>>,
  limit: number,
  onProgress?: (range_match: RegExpMatchArray, resp: Response) => void
): Promise<void> => {
  return processWithConcurrencyLimit(
    tasks,
    limit,
    onProgress
      ? async ([range_match, response]: [RegExpMatchArray, Response]) => onProgress(range_match, response)
      : undefined
  );
};