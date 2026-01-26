<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue';
import { Button, Progress, message, Card, Typography, Space } from 'ant-design-vue';
import { Download, FileText, HardDrive } from 'lucide-vue-next';
import { downloadFile, fetchJsonWithRetry, fetchWithRetry } from '@/utils/requests';
import { processDownloadWithConcurrencyLimit } from '@/utils/asyncPool';

const { Title, Text } = Typography;

const downloadProgress = ref(0);
const isDownloading = ref(false);
const isFinished = ref(false);
const fileName = ref('');
const fileSize = ref(0); // with units
const requiresCode = ref(false);
const codeDigits = ref<string[]>(['', '', '', '', '']);
const codeInputs = ref<Array<HTMLInputElement | null>>([]);
const activeFileId = ref<string | null>(null);

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const focusInput = (index: number) => {
  const el = codeInputs.value[index];
  if (el) {
    el.focus();
    el.select();
  }
};

const sanitizeInput = (value: string) => value.toLowerCase().replace(/[^0-9a-z]/g, '');

const trySubmitCode = () => {
  const code = codeDigits.value.join('');
  if (code.length === 5) {
    window.location.href = `/${code}/file`;
  }
};

const handleCodeInput = (event: Event, index: number) => {
  const target = event.target as HTMLInputElement;
  const sanitized = sanitizeInput(target.value);

  if (!sanitized) {
    codeDigits.value[index] = '';
    return;
  }

  const chars = sanitized.split('');
  codeDigits.value[index] = chars[0];

  let nextIndex = index + 1;
  for (let i = 1; i < chars.length && nextIndex < 5; i += 1, nextIndex += 1) {
    codeDigits.value[nextIndex] = chars[i];
  }

  if (nextIndex < 5) {
    focusInput(nextIndex);
  }
  trySubmitCode();
};

const handleCodeKeyDown = (event: KeyboardEvent, index: number) => {
  if (event.key === 'Backspace' && !codeDigits.value[index] && index > 0) {
    focusInput(index - 1);
  }
};

const handleCodePaste = (event: ClipboardEvent) => {
  const text = event.clipboardData?.getData('text') ?? '';
  const sanitized = sanitizeInput(text).slice(0, 5);
  if (!sanitized) return;

  const chars = sanitized.split('');
  for (let i = 0; i < 5; i += 1) {
    codeDigits.value[i] = chars[i] || '';
  }

  nextTick(() => {
    if (sanitized.length >= 5) {
      trySubmitCode();
    } else {
      focusInput(sanitized.length);
    }
  });
};

const handleGetFile = async () => {
  if (!activeFileId.value) {
    message.warning('请先输入有效的 5 位 ID');
    return;
  }

  isDownloading.value = true;
  downloadProgress.value = 0;

  let start = 0;
  let fileData: Map<number, Uint8Array> = new Map();
  let downloadedBytes = 0;

  const fileId = activeFileId.value;

  const downloadViaHttp = async () => {
    // Create an array to hold all chunk download promises
    const downloadPromises: Array<() => Promise<any>> = [];

    // Add chunks to download (in 1MB chunks)
    while (start < fileSize.value) {
      const chunkEnd = Math.min(start + 1024 * 1024, fileSize.value) - 1;
      const currentStart = start; // 保存当前start值的快照

      // Create a function that returns a promise for this chunk download
      // Pass fileId and the byte range start, but not fileName since it's not needed for the request
      downloadPromises.push(() => downloadFile(fileId, currentStart, fileName));

      if (chunkEnd === fileSize.value - 1) break;
      start += 1024 * 1024;
    }

    // Process downloads with a concurrency limit of 4
    try {
      await processDownloadWithConcurrencyLimit(downloadPromises, 4, async (rangeMatch: RegExpMatchArray, response: Response) => {
        const rangeStart = parseInt(rangeMatch[1]);

        // Get the file data
        const data = await response.arrayBuffer();
        const chunk = new Uint8Array(data);
        fileData.set(rangeStart, chunk);

        // Update progress - 累加当前chunk的大小而不是重新计算所有chunks
        downloadedBytes += chunk.length;

        // Only update progress if we have a valid totalSize
        if (fileSize.value > 0) {
          downloadProgress.value = Math.round((downloadedBytes / fileSize.value) * 100);
        }
      });
    } catch (error) {
      message.error('下载过程中发生错误: ' + (error instanceof Error ? error.message : '未知错误'));
      isDownloading.value = false;
      return;
    }

    await new Promise<void>(resolve => { setTimeout(resolve, 500)});

    // Combine all chunks and save the file
    if (fileData.size > 0) {
      try {
        // Combine all Uint8Array chunks into a single Uint8Array
        const combinedData = new Uint8Array(fileSize.value);
        let offset = 0;
        const sortedChunks = Array.from(fileData.entries()).sort((a, b) => a[0] - b[0]);

        for (const [start, chunk] of sortedChunks) {
          if (start !== offset) {
            message.error(`文件 ${fileName.value} 中有缺失的块，请重新上传`);
            return;
          }
          combinedData.set(chunk, offset);
          offset += chunk.length;
        }

        // Create a Blob from the combined data
        const blob = new Blob([combinedData], { type: 'application/octet-stream' });

        // Create a download link and trigger the download
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = fileName.value || 'downloaded_file';
        document.body.appendChild(a);
        a.click();

        // Clean up
        document.body.removeChild(a);
        URL.revokeObjectURL(url);

        message.success('文件下载完成!');

        // Send download completion signal to server
        try {
          const response = await fetchWithRetry(
            `/api/fileflow/${fileId}/done`,
            {
              method: 'PUT',
              headers: {
                'Content-Type': 'application/json',
              },
              body: JSON.stringify({})
            },
            { timeoutMs: 6000, retries: 2 },
          );

          if (!response.ok) {
            message.warning('无法通知服务器下载完成，但文件已成功下载');
          }
        } catch (error) {
          message.warning('无法通知服务器下载完成，但文件已成功下载');
        }
      } catch (error) {
        message.error('保存文件时发生错误: ' + (error instanceof Error ? error.message : '未知错误'));
      }
    } else {
      message.error('没有下载到任何文件数据');
    }

    isDownloading.value = false;
    isFinished.value = true;
  };

  await downloadViaHttp();
}

onMounted(async () => {
  if (localStorage.getItem("rid") == null || localStorage.getItem("rid") == "" || localStorage.getItem("rid") == undefined) {
    localStorage.setItem("rid", Math.random().toString(36).slice(2, 10));
  }

  const segments = window.location.pathname.split('/').filter(Boolean);
  if (segments.length === 0 || segments[0].length !== 5) {
    requiresCode.value = true;
    await nextTick();
    focusInput(0);
    return;
  }

  activeFileId.value = segments[0];

  // Get the file info from status API
  try {
    const { data: statusData, response } = await fetchJsonWithRetry<{ success?: boolean; data?: { file_name?: string; file_size?: number; } }>(
      `/api/fileflow/${activeFileId.value}/status`,
      { method: 'get' },
      { timeoutMs: 8000, retries: 2 },
    );

    if (!response.ok || !statusData?.success || !statusData.data) {
      message.error('获取文件信息失败: ' + ((statusData as any)?.message || '未知错误'));
      isDownloading.value = false;
      return;
    }

    // Get file info from status data
    const fileInfo = statusData.data;
    fileName.value = fileInfo.file_name || '未知文件';
    fileSize.value = fileInfo.file_size || 0;

  } catch (error: unknown) {
    message.error(`获取文件信息失败: ${(error as Error).message}`);
    isDownloading.value = false;
  }
});
</script>

<template>
  <div class="download-page">
    <Card v-if="requiresCode" class="code-card">
      <Space direction="vertical" size="large" style="width: 100%">
        <div class="header">
          <Download :size="48" :stroke-width="1.5" class="download-icon" />
          <Title :level="3" style="margin-bottom: 0;">输入 5 位 ID</Title>
          <Text type="secondary">输入接收方提供的 5 位代码即可进入下载</Text>
        </div>

        <div class="code-inputs" @paste.prevent="handleCodePaste">
          <input
            v-for="(_, index) in codeDigits"
            :key="index"
            :ref="(el) => codeInputs[index] = el as HTMLInputElement | null"
            type="text"
            inputmode="text"
            maxlength="1"
            class="code-box"
            v-model="codeDigits[index]"
            @input="(e) => handleCodeInput(e, index)"
            @keydown="(e) => handleCodeKeyDown(e, index)"
          />
        </div>

        <Button type="primary" size="large" block :disabled="codeDigits.join('').length !== 5" @click="trySubmitCode">
          进入下载
        </Button>
      </Space>
    </Card>

    <Card v-else class="download-card">
      <Space direction="vertical" size="large" style="width: 100%">
        <div class="header">
          <Download :size="48" :stroke-width="1.5" class="download-icon" />
          <Title :level="3" style="margin-bottom: 0;">文件下载</Title>
          <Text type="secondary">准备接收通过 FileFlow 分享的文件</Text>
        </div>

        <div class="file-info" v-if="fileName || isDownloading">
          <FileText class="file-icon" />
          <div class="file-details">
            <Text strong>{{ fileName || '未知文件' }}</Text>
            <Text type="secondary" v-if="fileSize">{{ formatBytes(fileSize) }}</Text>
          </div>
        </div>

        <Button type="primary" size="large" :loading="isDownloading" :disabled="isDownloading || isFinished"
          @click="handleGetFile" class="download-button">
          <template #icon>
            <HardDrive />
          </template>
          {{ isFinished ? '下载完成' : isDownloading ? '下载中...' : '开始下载' }}
        </Button>

        <div v-if="isDownloading" class="progress-container">
          <Progress :percent="downloadProgress" size="small" />
          <Text type="secondary">{{ downloadProgress }}% 已完成</Text>
        </div>

        <div class="instructions">
          <Title :level="5">如何使用:</Title>
          <ul>
            <li>确保分享链接来自可信来源</li>
            <li>点击"开始下载"按钮开始接收文件</li>
            <li>文件将自动保存到您的默认下载目录</li>
            <li>下载过程中请勿关闭此页面</li>
          </ul>
        </div>
      </Space>
    </Card>
  </div>
</template>

<style scoped>
.download-page {
  padding: 20px;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  background: linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%);
}

.code-card {
  width: 100%;
  max-width: 520px;
  border-radius: 12px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.12);
}

.code-inputs {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 12px;
}

.code-box {
  width: 100%;
  height: 56px;
  border-radius: 10px;
  border: 1px solid #d9d9d9;
  text-align: center;
  font-size: 24px;
  font-weight: 600;
  transition: border-color 0.2s, box-shadow 0.2s;
}

.code-box:focus {
  outline: none;
  border-color: #1890ff;
  box-shadow: 0 0 0 3px rgba(24, 144, 255, 0.16);
}

.download-card {
  width: 100%;
  max-width: 500px;
  border-radius: 12px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.header {
  text-align: center;
  padding: 20px 0;
}

.download-icon {
  color: #1890ff;
  margin-bottom: 16px;
}

.file-info {
  display: flex;
  align-items: center;
  padding: 16px;
  background-color: #f0f8ff;
  border-radius: 8px;
  margin: 16px 0;
}

.file-icon {
  color: #1890ff;
  margin-right: 12px;
}

.file-details {
  flex: 1;
}

.file-details :deep(.ant-typography) {
  display: block;
}

.download-button {
  display: flex;
  justify-content: center;
  width: 100%;
  height: 48px;
  font-size: 16px;
  margin: 16px 0;
  gap: 16px;
}

.progress-container {
  width: 100%;
  text-align: center;
}

.instructions {
  background-color: #fafafa;
  border-radius: 8px;
  padding: 16px;
  margin-top: 16px;
}

.instructions ul {
  padding-left: 20px;
  margin: 8px 0 0;
}

.instructions li {
  margin-bottom: 8px;
  color: #555;
}

:deep(.ant-progress-inner) {
  border-radius: 10px;
}

:deep(.ant-progress-bg) {
  border-radius: 10px;
}
</style>
