<script setup lang="ts">
import { ref } from 'vue';
import { Button, Progress, message, Card, Typography, Space } from 'ant-design-vue';
import { onMounted } from 'vue';
import { Download, FileText, HardDrive } from 'lucide-vue-next';
import { downloadFile } from '@/utils/requests';
import { processDownloadWithConcurrencyLimit } from '@/utils/asyncPool';

const { Title, Text } = Typography;

const downloadProgress = ref(0);
const isDownloading = ref(false);
const isFinished = ref(false);
const fileName = ref('');
const fileSize = ref(0); // with units

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const handleGetFile = async () => {
  isDownloading.value = true;
  downloadProgress.value = 0;
  
  let start = 0;
  let fileData: Map<number, Uint8Array> = new Map();
  let downloadedBytes = 0;

  // Extract the file ID from the URL
  const pathParts = window.location.pathname.split('/');
  const fileId = pathParts[1]; // Assumes URL format is /{id}/file

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
        if(start !== offset){
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
        const pathParts = window.location.pathname.split('/');
        const fileId = pathParts[1]; // Assumes URL format is /{id}/file
        
        const response = await fetch(`/api/${fileId}/done`, {
          method: 'PUT',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({})
        });
        
        if (response.ok) {
          // Download completion signal sent successfully
        } else {
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
}

onMounted(async () => {
  if (localStorage.getItem("rid") == null || localStorage.getItem("rid") == "" || localStorage.getItem("rid") == undefined) {
    localStorage.setItem("rid", Math.random().toString(36).substring(16));
  }

  // Get the file info from status API
  try {
    // Extract the file ID from the URL
    const pathParts = window.location.pathname.split('/');
    const fileId = pathParts[1]; // Assumes URL format is /{id}/file

    const response = await fetch(`/api/${fileId}/status`);
    const statusData = await response.json();

    if (!statusData.success) {
      message.error('获取文件信息失败: ' + (statusData.message || '未知错误'));
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
    return;
  }

});
</script>

<template>
  <div class="download-page">
    <Card class="download-card">
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