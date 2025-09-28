<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { Upload as UploadIcon, FileText, HardDrive } from 'lucide-vue-next';
import { message, Button, Upload, Progress, Card, Typography, Space, Alert } from 'ant-design-vue';
import type { UploadProps } from 'ant-design-vue';
import { uploadFile } from '@/utils/requests';
import { processUploadWithConcurrencyLimit } from '@/utils/asyncPool';

const CHUNK_SIZE = 1024 * 1024;

const fileList = ref<UploadProps['fileList']>([]);
const uploadState = ref<'idle' | 'pending' | 'processing' | 'finished'>('idle');

const accessId = ref<string | null>(null);
const uploadProgress = ref(0);
const maxPollCount = 120; // 最多等待120秒
const uploadedLength = ref(0);
const is_online = ref(false);
const remainingPolls = ref(maxPollCount);

const intervalRef = ref<number | undefined>(undefined)

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
  const newFileList = fileList.value!.slice();
  newFileList.splice(index, 1);
  fileList.value = newFileList;
};

const beforeUpload: UploadProps['beforeUpload'] = file => {
  // Check if a file is already selected
  if (fileList.value && fileList.value.length > 0) {
    message.warning('只能上传一个文件');
    return false;
  }

  fileList.value = [file];
  return false;
};

const getAccessId = async () => {
  if (fileList.value === undefined || fileList.value.length === 0) {
    message.warning('请先选择一个文件');
    return;
  }

  try {
    const response = await fetch(`/api/get_id?file_name=${fileList.value[0].name}&file_size=${fileList.value[0].size}`);
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
      const statusResponse = await fetch(`/api/${accessId.value}/status`);
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

    for (const file of fileList.value || []) {
      // Access the native File object from the Ant Design Vue file object
      const nativeFile: File = (file as any).originFileObj ? (file as any).originFileObj : file;

      const fileSize = file.size || 0;

      // Calculate number of chunks
      const chunks = Math.ceil(fileSize / CHUNK_SIZE);

      message.info(`开始上传文件: ${file.name} (${formatBytes(fileSize)})`);

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
          filename: file.name,
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
    }

    uploadState.value = 'finished';
    message.success('文件上传成功！');

    // Reset everything after a short delay to allow user to see the success message
    setTimeout(() => {
      resetUpload(false);
    }, 1000);
  } catch (error) {
    resetUpload(true);
    message.error("上传失败: " + (error as Error).message);
  }
};


const sayHello = () => {
  fetch('/api/hello')
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
    <Card class="upload-card">
      <Space direction="vertical" size="large" style="width: 100%">
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
          <Upload :file-list="fileList" :before-upload="beforeUpload" @remove="handleRemove" draggable
            :disabled="uploadState !== 'idle'">
            <template #itemRender="{ file, actions }">
              <Space>
              </Space>
            </template>
            <Button  :disabled="uploadState !== 'idle'">
              选择要上传的文件
            </Button>
          </Upload>

          <div v-if="fileList && fileList.length > 0" class="file-info">
            <FileText class="file-icon" />
            <div class="file-details">
              <Text strong>{{ fileList[0].name }}</Text>
              <Text type="secondary">{{ fileList[0].size ? formatBytes(fileList[0].size) : '未知大小' }}</Text>
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
          :loading="uploadState === 'pending' || uploadState === 'processing'" @click="handleUpload" class="upload-button">
          <template #icon>
            <HardDrive />
          </template>
          {{ uploadState === 'processing' ? '上传中...' : uploadState === 'pending' ? '等待对方接收' :  '获取链接并上传' }}
        </Button>

        <div v-if="uploadState === 'processing'" class="progress-container">
          <Progress :percent="uploadProgress" size="small" />
          <Text type="secondary">{{ uploadProgress }}% 已上传</Text>
        </div>

        <div class="instructions">
          <Title :level="5">使用说明:</Title>
          <ul>
            <li>点击"选择要上传的文件"选择一个文件</li>
            <li>点击"获取链接并上传"按钮获取分享链接并开始上传</li>
            <li>将链接发送给文件接收方</li>
            <li>等待接收方访问链接后，文件将自动开始上传</li>
            <li>上传过程中请勿关闭页面</li>
          </ul>
        </div>
      </Space>
    </Card>
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
  width: 100%;
  max-width: 500px;
  border-radius: 12px;
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

.file-info {
  display: flex;
  align-items: center;
  padding: 16px;
  background-color: #f0f8ff;
  border-radius: 8px;
  margin-top: 16px;
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
</style>
