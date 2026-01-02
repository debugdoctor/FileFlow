<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { Upload as UploadIcon, FileText, HardDrive, X } from 'lucide-vue-next';
import { message, Button, Upload, Progress, Card, Typography, Space, Alert } from 'ant-design-vue';
import type { UploadProps } from 'ant-design-vue';
import { uploadFile } from '@/utils/requests';
import { sendViaWebRtc } from '@/utils/webrtc';
import { processUploadWithConcurrencyLimit } from '@/utils/asyncPool';
import JSZip from 'jszip';

const CHUNK_SIZE = 1024 * 1024;

const fileList = ref<UploadProps['fileList']>([]);
const uploadState = ref<'idle' | 'pending' | 'processing' | 'finished'>('idle');
const isFolderUpload = ref(false);
const zipFile = ref<File | null>(null);

const accessId = ref<string | null>(null);
const uploadProgress = ref(0);
const maxPollCount = 120; // 最多等待120秒
const uploadedLength = ref(0);
const is_online = ref(false);
const remainingPolls = ref(maxPollCount);

const intervalRef = ref<ReturnType<typeof setInterval> | undefined>(undefined)
const uploadRef = ref<any>(null)

const HOST = window.location.origin;

const { Title, Text } = Typography;

interface UploadInfo {
  is_final: number;
  filename: string;
  start: number;
  end: number;
  total: number;
}

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const handleRemove: UploadProps['onRemove'] = file => {
  const index = fileList.value!.indexOf(file);
  const newFiles = fileList.value!.slice();
  newFiles.splice(index, 1);
  fileList.value = newFiles;
};

const beforeUpload: UploadProps['beforeUpload'] = (file) => {
  // Handle both files and directories
  if (file) {
    if (!fileList.value) fileList.value = [];
    fileList.value.push(file);

    // Check if it's a directory upload
    if ((file as any).webkitRelativePath) {
      isFolderUpload.value = true;
    } else {
      // For multiple files, we'll create a zip
      if (fileList.value.length > 1) {
        isFolderUpload.value = true;
      }
    }
  }
  return false;
};

const createZipFromFolder = async () => {
  if (!fileList.value || fileList.value.length === 0) return null;

  const zip = new JSZip();
  const folderName = getFolderName();

  // Add all files to the zip
  for (const fileItem of fileList.value) {
    const originFile = (fileItem as any).originFileObj || fileItem;

    if (originFile && originFile.webkitRelativePath) {
      // Add file with its relative path to maintain folder structure
      const relativePath = originFile.webkitRelativePath;
      zip.file(relativePath, originFile);
    } else {
      // For files without relative path (multiple files upload), add them to root
      zip.file(originFile.name, originFile);
    }
  }

  try {
    // Generate the zip file
    const zipBlob = await zip.generateAsync({ type: 'blob' });
    const zipFileName = `${folderName}.zip`;
    return new File([zipBlob], zipFileName, { type: 'application/zip' });
  } catch (error) {
    console.error('Error creating zip file:', error);
    message.error('创建压缩文件失败');
    return null;
  }
};

const generateRandomZipName = () => {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  let result = '';
  for (let i = 0; i < 16; i++) {
    result += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return result;
};

const getFolderName = () => {
  if (!fileList.value || fileList.value.length === 0) return 'folder';

  const firstFile = fileList.value[0];
  const originFile = (firstFile as any).originFileObj || firstFile;

  if (originFile && originFile.webkitRelativePath) {
    // Extract folder name from the first file's relative path
    const pathParts = originFile.webkitRelativePath.split('/');
    return pathParts[0] || 'folder';
  }

  // For multiple files without folder structure, use random name
  if (fileList.value.length > 1) {
    return generateRandomZipName();
  }

  // For single file, use the filename without extension
  const fileName = originFile.name || 'file';
  const lastDotIndex = fileName.lastIndexOf('.');
  return lastDotIndex > 0 ? fileName.substring(0, lastDotIndex) : fileName;
};

const handleChange: UploadProps['onChange'] = ({ fileList: newFileList }) => {
  // Update the file list when files are added or removed
  if (newFileList && newFileList.length > 0) {
    fileList.value = newFileList;

    // Check if any files have webkitRelativePath (indicating folder upload)
    const hasFolderFiles = newFileList.some(file => (file as any).originFileObj?.webkitRelativePath);
    const hasMultipleFiles = newFileList.length > 1;

    // Set isFolderUpload to true for folder uploads or multiple files
    isFolderUpload.value = hasFolderFiles || hasMultipleFiles;
    
    // Clear Ant Design's internal file list after processing
    setTimeout(() => {
      if (uploadRef.value) {
        uploadRef.value.fileList = [];
      }
    }, 0);
  }
};

const handleDrop = (e: DragEvent) => {
  // Handle drop event for folder structure
  const items = e.dataTransfer?.items;
  if (items) {
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.kind === 'file') {
        const entry = item.webkitGetAsEntry();
        if (entry) {
          processEntry(entry);
        }
      }
    }
  }
};

const processEntry = (entry: any) => {
  if (entry.isFile) {
    entry.file((file: File) => {
      // Add file to file list with relative path
      const fileWithPath = Object.assign(file, {
        webkitRelativePath: entry.fullPath.replace(/^\//, '')
      });
      if (!fileList.value) fileList.value = [];
      fileList.value.push(fileWithPath as any);
    });
  } else if (entry.isDirectory) {
    const reader = entry.createReader();
    reader.readEntries((entries: any[]) => {
      entries.forEach(processEntry);
    });
  }
};

const getAccessId = async () => {
  if (fileList.value === undefined || fileList.value.length === 0) {
    message.warning('请先选择一个文件或文件夹');
    return;
  }

  // Create zip file if it's a folder upload
  if (isFolderUpload.value) {
    zipFile.value = await createZipFromFolder();
    if (!zipFile.value) {
      message.error('创建压缩文件失败');
      return;
    }
  }

  try {
    const fileToUpload = isFolderUpload.value ? zipFile.value : fileList.value[0];
    if (!fileToUpload) {
      message.error('没有可上传的文件');
      return;
    }
    const response = await fetch(`/api/fileflow/id?file_name=${fileToUpload.name}&file_size=${fileToUpload.size}`);
    const body = await response.json();
    accessId.value = body.data.id;
    message.success('AccessId 获取成功，请将链接分享给接收方');
    return accessId.value;
  } catch (error) {
    message.error('获取 AccessId 失败');
    throw error;
  }
};

const resetUpload = (by_error: boolean) => {
  // Reset all states after upload completion
  remainingPolls.value = 0;
  uploadProgress.value = 0;
  uploadedLength.value = 0;
  uploadState.value = 'idle';
  accessId.value = null;
  zipFile.value = null;
  isFolderUpload.value = false;
  if (!by_error) {
    fileList.value = [];
  }
};

const handleUpload = async () => {
  // If we don't have an accessId yet, get one
  if (!accessId.value) {
    try {
      await getAccessId();
    } catch (error) {
      return;
    }
  }

  uploadState.value = 'pending';
  uploadProgress.value = 0;

  message.info('正在等待接收方连接...');
  remainingPolls.value = maxPollCount;

  try {
    // Polling to check if the access code is in use
    let isUsing = false;
    let pollCount = 0;

    while (!isUsing && pollCount < maxPollCount) {
      const statusResponse = await fetch(`/api/fileflow/${accessId.value}/status`);
      const statusData = await statusResponse.json();

      // Check if is_using is true
      if (statusData.success && statusData.data && statusData.data.is_using) {
        isUsing = true;
        uploadState.value = 'processing';
        message.success('接收方已连接，开始上传文件...');
      } else {
        // Wait for 1 second before checking again
        await new Promise(resolve => setTimeout(resolve, 1000));
        pollCount++;
        remainingPolls.value = maxPollCount - pollCount;
        uploadState.value = 'pending';
      }
    }

    if (!isUsing) {
      message.error('等待接收方连接超时');
      resetUpload(true);
      return;
    }

    // Determine which file to upload (original file or zip file)
    const fileToUpload = isFolderUpload.value ? zipFile.value : (fileList.value ? fileList.value[0] : null);

    if (!fileToUpload) {
      message.error('没有可上传的文件');
      return;
    }

    // Access the native File object from the Ant Design Vue file object
    const nativeFile: File = isFolderUpload.value ? fileToUpload : ((fileToUpload as any).originFileObj ? (fileToUpload as any).originFileObj : fileToUpload);

    const fileSize = nativeFile.size || 0;
    const fileName = nativeFile.name;

    const uploadViaHttp = async () => {
      // Calculate number of chunks
      const chunks = Math.ceil(fileSize / CHUNK_SIZE);

      message.info(`开始上传${isFolderUpload.value ? '压缩' : ''}文件: ${fileName} (${formatBytes(fileSize)})`);

      // Create an array to hold all chunk upload promises
      const uploadPromises: Array<() => Promise<any>> = [];

      // Create all chunk upload functions
      for (let i = 0; i < chunks; i++) {
        const start = i * CHUNK_SIZE;
        const end = Math.min(start + CHUNK_SIZE, fileSize);
        const chunkIndex = i; // 保存当前索引的快照

        // Slice the file
        const chunk = nativeFile.slice(start, end);

        // Prepare form data
        const formData = new FormData();
        formData.append('info', JSON.stringify({
          filename: fileName,
          start: start,
          end: end - 1,
          total: fileSize,
        } as UploadInfo));
        formData.append('file', chunk);

        // Create a function that returns a promise for this chunk upload
        uploadPromises.push(() => uploadFile(formData, accessId.value, chunkIndex, chunk.size));
      }

      // Process uploads with a concurrency limit of 4
      await processUploadWithConcurrencyLimit(uploadPromises, 4, (seq_len) => {
        uploadedLength.value += seq_len;
        uploadProgress.value = Math.round((uploadedLength.value / fileSize) * 100);
      });
    };

    if (!accessId.value) {
      message.error('AccessId 为空，无法建立连接');
      resetUpload(true);
      return;
    }

    const p2pResult = await sendViaWebRtc(accessId.value, nativeFile, {
      onProgress: (percent) => {
        uploadProgress.value = percent;
      },
      onStatus: (status) => {
        if (status.startsWith('fallback')) {
          message.warning('P2P 失败，切换到服务器上传');
        }
      },
    });

    if (p2pResult.status === 'success') {
      uploadState.value = 'finished';
      message.success('P2P 发送完成！等待接收方下载完成...');
    } else {
      uploadedLength.value = 0;
      uploadProgress.value = 0;
      await uploadViaHttp();
      uploadState.value = 'finished';
      message.success('文件上传成功！等待接收方下载完成...');
    }

    let retry: number = 0;
    // Start polling to check if download is complete
    const checkDownloadComplete = async () => {
      if (retry > 20) {
        message.error('未能得知文件是否被下载完成，确认下载完成则可以关闭窗口！');
        return;
      }

      try {
        const response = await fetch(`/api/fileflow/${accessId.value}/status`);
        const data = await response.json();

        if (data.success && data.data && data.data.done) {
          message.success('接收方已下载完成！');
          // Reset everything after a short delay
          setTimeout(() => {
            resetUpload(false);
          }, 1500);
        } else {
          // Continue polling every second
          setTimeout(checkDownloadComplete, 1000);
        }
      } catch (error) {
        message.warning('检查下载状态时出错，但将继续尝试');
        // Continue polling even on error
        setTimeout(checkDownloadComplete, 1000);
      }
    };

    // Start polling after a short delay
    setTimeout(checkDownloadComplete, 1000);
  } catch (error) {
    resetUpload(true);
    const errorMessage = error instanceof Error ? error.message : '未知错误';
    message.error("上传失败: " + errorMessage);
  }
};


const sayHello = () => {
  fetch('/api/fileflow/hello')
    .then(response => {
      if (response.status === 200) {
        is_online.value = true;
      } else {
        is_online.value = false;
      }
    })
    .catch(() => {
      is_online.value = false;
    });
};

onMounted(() => {
  sayHello();
  intervalRef.value = setInterval(() => {
    sayHello();
  }, 5000);
});

onUnmounted(() => {
  if (intervalRef.value) {
    clearInterval(intervalRef.value);
  }
});
</script>

<template>
  <div class="file-upload-container">
    <div class="upload-card">
      <Space direction="vertical" size="large" style="width: 100%; max-width: 600px;">
        <div class="header">
          <UploadIcon :size="48" :stroke-width="1.5" class="upload-icon" />
          <Title :level="3" style="margin-bottom: 0;">文件上传</Title>
          <Text type="secondary">通过 FileFlow 快速安全地分享文件</Text>
        </div>

        <Alert type="info" show-icon message="服务状态" :description="is_online ? '已连接到服务器，可以正常上传文件' : '无法连接到服务器，请检查网络连接'"
          :class="is_online ? 'status-online' : 'status-offline'" />

        <div v-if="uploadState === 'pending'" class="wait-time-container">
          <Text type="warning">等待接收方连接中... 剩余等待时间: {{ remainingPolls }} 秒</Text>
        </div>

        <div class="file-upload-area">
          <Upload.Dragger ref="uploadRef" :file-list="fileList" :before-upload="beforeUpload" @remove="handleRemove"
            :disabled="uploadState !== 'idle'" :multiple="true" :directory="true" name="file" :show-upload-list="false"
            @change="handleChange" @drop="handleDrop">
            <div class="upload-area-wrapper">
              <p class="ant-upload-text">点击或拖拽文件到此区域上传</p>
              <p class="ant-upload-hint">
                支持单个文件、多个文件或文件夹上传。请勿上传敏感数据。
              </p>
            </div>
          </Upload.Dragger>

          <div v-if="fileList && fileList.length > 0" class="file-info">
            <div v-for="file in fileList" class="file-item">
              <FileText class="file-icon" />
              <div class="file-details">
                <Text strong>{{ file.name }}</Text>
                <Text type="secondary">{{ file.size ? formatBytes(file.size) : '未知大小' }}</Text>
              </div>
              <Button type="text" size="small" @click="handleRemove(file)" class="remove-button">
                <X :size="16" />
              </Button>
            </div>
          </div>
        </div>

        <div class="access-id-section">
          <div v-if="accessId" class="access-id-display">
            <Text strong>分享链接:</Text>
            <div class="link-container">
              <Text>{{ `${HOST}/${accessId}/file` }}</Text>
            </div>
            <Text type="secondary">请将此链接发送给文件接收方</Text>
          </div>
        </div>

        <Button type="primary" size="large" :disabled="!fileList || fileList.length === 0 || uploadState !== 'idle'"
          :loading="uploadState === 'pending' || uploadState === 'processing'" @click="handleUpload"
          class="upload-button">
          <template #icon>
            <HardDrive />
          </template>
          {{ uploadState === 'processing' ? '上传中...' : uploadState === 'pending' ? '等待对方接收' : '获取链接并上传' }}
        </Button>

        <div v-if="uploadState === 'processing'" class="progress-container">
          <Progress :percent="uploadProgress" size="small" />
          <Text type="secondary">{{ uploadProgress }}% 已上传</Text>
        </div>

        <div class="instructions">
          <Title :level="5">使用说明:</Title>
          <ul>
            <li>点击上传区域或拖拽文件/文件夹到此区域</li>
            <li>支持单个文件、多个文件或整个文件夹上传</li>
            <li>多个文件或文件夹将自动打包成压缩文件</li>
            <li>点击"获取链接并上传"按钮获取分享链接并开始上传</li>
            <li>将链接发送给文件接收方</li>
            <li>等待接收方访问链接后，文件将自动开始上传</li>
            <li>上传过程中请勿关闭页面</li>
          </ul>
        </div>
      </Space>
    </div>
  </div>
</template>

<style scoped>
.file-upload-container {
  padding: 20px;
  display: flex;
  justify-content: center;
  align-items: center;
  min-height: 100vh;
  background: linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%);
}

.upload-card {
  display: flex;
  justify-content: center;
  padding: 24px;
  width: 100%;
  max-width: 800px;
  border-radius: 12px;
  background: #ffffff;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
}

.header {
  text-align: center;
  padding: 20px 0;
}

.upload-icon {
  color: #1890ff;
  margin-bottom: 16px;
}

.status-online {
  border-left: 4px solid #52c41a;
}

.status-offline {
  border-left: 4px solid #f5222d;
}

.file-upload-area {
  width: 100%;
}

.upload-area-wrapper {
  display: flex;
  flex-direction: column;
  justify-content: center;
  height: 256px;
}

.file-info {
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: flex-start;
  background-color: #e6f4ff;
  padding: 16px;
  margin-top: 16px;
  gap: 16px;
  max-height: 512px;
  overflow-y: auto;
}

.file-item {
  width: 100%;
  padding: 8px;
  display: inline-flex;
  align-items: center;
  background-color: #ffffff;
  border-radius: 8px;
  position: relative;
}

.file-icon {
  color: #1890ff;
  margin-right: 12px;
}

.file-details {
  flex: 1;
}

.remove-button {
  display: flex;
  align-items: center;
  color: #ff4d4f;
  opacity: 0.7;
  transition: opacity 0.2s;
}

.remove-button:hover {
  opacity: 1;
  background-color: #fff2f0;
}

.file-details :deep(.ant-typography) {
  display: block;
}

.access-id-section {
  width: 100%;
  text-align: center;
}

.access-id-display {
  margin-top: 16px;
  padding: 16px;
  background-color: #f0f8ff;
  border-radius: 8px;
  text-align: left;
}

.link-container {
  margin: 8px 0;
  padding: 8px;
  background-color: #fff;
  border-radius: 4px;
  word-break: break-all;
}

.upload-button {
  display: flex;
  justify-content: center;
  width: 100%;
  height: 48px;
  font-size: 16px;
  margin: 8px 0;
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

.ant-upload-drag-icon {
  color: #1890ff;
  margin-bottom: 16px;
}

.ant-upload-text {
  font-size: 16px;
  font-weight: 500;
  color: rgba(0, 0, 0, 0.85);
  margin-bottom: 8px;
}

.ant-upload-hint {
  color: rgba(0, 0, 0, 0.45);
  font-size: 14px;
}
</style>
