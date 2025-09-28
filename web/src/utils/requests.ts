import message from "ant-design-vue/es/message";
import type { Ref } from "vue";

const MAX_RETRIES = 3;


export const uploadFile = async (formData: FormData, accessId: string | null, i: number, seq_len: number): Promise<number> => {
    // Upload the file block with timeout mechanism
    let retries = 0;

    while (retries <= MAX_RETRIES) {
        try {
            // Create a timeout promise
            const timeoutPromise = new Promise((_, reject) => {
                setTimeout(() => reject(new Error('Request timeout')), 18000);
            });

            // Create the fetch promise
            const fetchPromise = fetch(`/api/${accessId}/upload`, {
                method: 'post',
                body: formData,
            });

            // Race the fetch promise against the timeout
            const response = await Promise.race([fetchPromise, timeoutPromise]) as Response;
            const body = await response.json();
            if (body.code !== 200) {
                throw new Error(`Upload failed for chunk ${i + 1} with status ${response.status}`);
            }

            // If successful, return the seq length
            return seq_len;
        } catch (error: unknown) {
            retries++;
            if (retries > MAX_RETRIES) {
                message.error("Upload failed. Please try again.");
                throw new Error(`Upload failed for chunk ${i + 1}: ${(error as Error).message}`);
            }
            // Wait a bit before retrying
            await new Promise(resolve => setTimeout(resolve, 500));
        }
    }
    
    // This line should never be reached, but it's needed to satisfy TypeScript
    throw new Error(`Upload failed for chunk ${i + 1} after ${MAX_RETRIES} retries`);
}

export const downloadFile = async (fileId: string, start: number, fileName: Ref<string>): Promise<[RegExpMatchArray, Response]> => {
    const response = await fetch(`/api/${fileId}/file?rid=${localStorage.getItem("rid")}&start=${start}`);

    // Get the Content-Range header
    const contentRange = response.headers.get('Content-Range');
    // Get the Content-Name header for filename
    const contentName = response.headers.get('Content-Name');
    if (contentName) {
        fileName.value = contentName;
    }

    if (contentRange) {
        // Parse the Content-Range header (format: "bytes start-end/total")
        const rangeMatch = contentRange.match(/bytes\s+(\d+)-(\d+)\/(\d+)/);
        if (rangeMatch) {
            return [rangeMatch, response];
        }

        throw new Error('Failed to fetch file');
    }

    throw new Error('Invalid URL');
}