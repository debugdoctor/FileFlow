<script setup lang="ts">
import { ref } from 'vue';
import { Button, Progress, message, Card, Typography, Space } from 'ant-design-vue';
import { onMounted } from 'vue';
import { Download, FileText, HardDrive } from 'lucide-vue-next';

const { Title, Text } = Typography;

const downloadProgress = ref(0);
const isDownloading = ref(false);
const isFinished = ref(false);
const fileName = ref('');
const fileSize = ref('');

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const getFile = async () => {
  isDownloading.value = true;
  downloadProgress.value = 0;
  
  let start = 0;
  let is_final = false;
  let fileData: Uint8Array[] = [];
  let totalSize = 0;
  let name = '';
  
  // Extract the file ID from the URL
  const pathParts = window.location.pathname.split('/');
  const fileId = pathParts[1]; // Assumes URL format is /{id}/file
  
  message.info('开始下载文件...');
  
  while (!is_final) {
    try {
      const response = await fetch(`/api/${fileId}/file?rid=${localStorage.getItem("rid")}&start=${start}`);
      
      // Get the Content-Range header
      const contentRange = response.headers.get('Content-Range');
      // Get the Content-Name header for filename
      const contentName = response.headers.get('Content-Name');
      if (contentName) {
        name = contentName;
        fileName.value = name;
      }
      
      if (contentRange) {
        // Parse the Content-Range header (format: "bytes start-end/total")
        const rangeMatch = contentRange.match(/bytes\s+(\d+)-(\d+)\/(\d+)/);
        if (rangeMatch) {
          const rangeStart = parseInt(rangeMatch[1]);
          const rangeEnd = parseInt(rangeMatch[2]);
          totalSize = parseInt(rangeMatch[3]);
          
          fileSize.value = formatBytes(totalSize);
          
          // Update progress
          downloadProgress.value = Math.round(((rangeEnd + 1) / totalSize) * 100);
          
          // Check if this is the final chunk
          is_final = rangeEnd === totalSize - 1;
          start = rangeEnd + 1;
        }
      }
      
      // Get the file data
      const data = await response.arrayBuffer();
      fileData.push(new Uint8Array(data));
      
      // If we don't have a Content-Range header, we assume it's the final chunk
      if (!contentRange) {
        is_final = true;
        downloadProgress.value = 100;
      }
    } catch (error: unknown) {
      message.error(`下载文件时出错: ${(error as Error).message}`);
      is_final = true;
    }
  }
  
  // Combine all chunks and save the file
  if (fileData.length > 0) {
    // Combine all Uint8Array chunks into a single Uint8Array
    const combinedData = new Uint8Array(totalSize);
    let offset = 0;
    for (const chunk of fileData) {
      combinedData.set(chunk, offset);
      offset += chunk.length;
    }
    
    // Create a Blob from the combined data
    const blob = new Blob([combinedData], { type: 'application/octet-stream' });
    
    // Create a download link and trigger the download
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = name || 'downloaded_file';
    document.body.appendChild(a);
    a.click();
    
    // Clean up
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    
    message.success('文件下载完成!');
  }
  
  isDownloading.value = false;
  isFinished.value = true;
}

onMounted(() => {
  if (localStorage.getItem("rid") == null || localStorage.getItem("rid") == "" || localStorage.getItem("rid") == undefined) {
    localStorage.setItem("rid", Math.random().toString(36).substring(16));
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
            <Text type="secondary" v-if="fileSize">{{ fileSize }}</Text>
          </div>
        </div>
        
        <Button 
          type="primary" 
          size="large" 
          :loading="isDownloading" 
          :disabled="isDownloading || isFinished"
          @click="getFile" 
          class="download-button"
        >
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