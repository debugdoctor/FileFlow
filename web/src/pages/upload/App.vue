<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { Circle } from 'lucide-vue-next';
import { message } from 'ant-design-vue';
import type { UploadProps } from 'ant-design-vue';

const CHUNK_SIZE = 1024 * 1024;

const fileList = ref<UploadProps['fileList']>([]);
const uploading = ref<boolean>(false);

const accessId = ref<string | null>(null);
const uploadStatus = ref('idle');
const uploadProgress = ref(0);
const pollingInterval = ref(null);
const uploadResult = ref(null);
const is_online = ref(false);

const intervanalRef = ref<number | undefined>(undefined)

const HOST = window.location.origin;

interface UploadInfo {
  is_final: number;
  filename: string;
  start: number;
  end: number;
  total: number;
}

const resetUpload = () => {
  fileList.value = [];
  accessId.value = null;
  uploadStatus.value = 'idle';
  uploadProgress.value = 0;
  uploadResult.value = null;
  if (pollingInterval.value) {
    clearInterval(pollingInterval.value);
    pollingInterval.value = null;
  }
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

const handleUpload = async () => {
  uploading.value = true;
  uploadProgress.value = 0;
  
  try {
    // Polling to check if the access code is in use
    let isUsing = false;
    while (!isUsing) {
      const statusResponse = await fetch(`/api/${accessId.value}/status`);
      const statusData = await statusResponse.json();
      
      // Check if is_using is true
      if (statusData.success && statusData.data.is_using) {
        isUsing = true;
        console.log("Access code is now in use, proceeding with upload");
      } else {
        // Wait for 1 second before checking again
        await new Promise(resolve => setTimeout(resolve, 1000));
      }
    }

    for (const file of fileList.value || []) {
      // Access the native File object from the Ant Design Vue file object
      const nativeFile: File = (file as any).originFileObj ? (file as any).originFileObj : file;
      
      const fileSize = file.size || 0;
      let uploadedBytes = 0;
      
      // Calculate number of chunks
      const chunks = Math.ceil(fileSize / CHUNK_SIZE);
      
      // Upload each chunk
      for (let i = 0; i < chunks; i++) {
        const start = i * CHUNK_SIZE;
        const end = Math.min(start + CHUNK_SIZE, fileSize);
        const isFinal = i === chunks - 1 ? 1 : 0;
        
        // Slice the file
        const chunk = nativeFile.slice(start, end);
        
        // Prepare form data
        const formData = new FormData();
        formData.append('info', JSON.stringify({
          is_final: isFinal,
          filename: file.name,
          start: start,
          end: end,
          total: fileSize,
        } as UploadInfo));
        formData.append('file', chunk);
        
        // Upload the file block with retry mechanism
        let retries = 0;
        const maxRetries = 5;
        let success = false;
        let lastError: Error | null = null;
        
        while (retries <= maxRetries && !success) {
          try {
            const response = await fetch(`/api/${accessId.value}/upload`, {
              method: 'post',
              body: formData,
            });
            const body = await response.json();
            if (body.code === 200) {
              success = true;
            } else {
              throw new Error(`Upload failed for chunk ${i + 1} with status ${response.status}`);
            }
          } catch (error) {
            lastError = error as Error;
            retries++;
            if (retries <= maxRetries) {
              // Wait before retrying (exponential backoff)
              await new Promise(resolve => setTimeout(resolve, 1000 * retries));
            }
          }
        }
        
        if (!success) {
          throw new Error(`Upload failed for chunk ${i + 1} after ${maxRetries} retries: ${lastError?.message}`);
        }
        
        // Update progress
        uploadedBytes += (end - start);
        uploadProgress.value = Math.round((uploadedBytes / Math.max(fileSize, 1)) * 100);
      }
    }
    
    fileList.value = [];
    uploading.value = false;
    uploadProgress.value = 100;
    message.success('Upload successful.');
  } catch (error) {
    console.error('Upload error:', error);
    uploading.value = false;
    message.error('Upload failed.');
  }
};

const getAccessId = async () => {
  if (fileList.value === undefined || fileList.value.length === 0) {
    message.warning('请先选择一个文件');
    return;
  }

  try {
    fetch('/api/get_id')
      .then(response => response.json())
      .then(body => {
        accessId.value = body.data.id;
      });

  } catch (error) {
    uploadStatus.value = 'error';
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
    .catch(error => {
      console.error('Error fetching /api/hello:', error);
      is_online.value = false;
    });
};

onMounted(() => {
  sayHello();
  intervanalRef.value = setInterval(() => {
    sayHello();
  }, 5000);
});

onUnmounted(() => {
  if (intervanalRef.value) {
    clearInterval(intervanalRef.value);
  }
});
</script>

<template>
  <div class="file-upload-container">
    <div>
      <Circle :fill="is_online ? '#52c41a' : '#f5222d'" :color="'#0000'" :size="16" />
      <span>{{ is_online ? '服务器已连接' : '未连接服务器' }}</span>
    </div>
    <a-upload :file-list="fileList" :before-upload="beforeUpload" @remove="handleRemove">
      <a-button icon="upload">选择文件</a-button>
    </a-upload>
    <div class="access-id-display">
      <span v-if="accessId">{{ `${HOST}/${accessId}/file` }}</span>
      <a-button @click="getAccessId">获取AccessId</a-button>
    </div>
    <a-button type="primary" :disabled="fileList === undefined || fileList.length === 0 || uploading"
      :loading="uploading" @click="handleUpload" style="margin-top: 16px;">
      上传
    </a-button>
    
    <!-- Add progress bar -->
    <div v-if="uploading" class="progress-container">
      <a-progress :percent="uploadProgress" />
    </div>
  </div>
</template>

<style scoped>
.file-upload-container {
  padding: 20px;
  max-width: 600px;
  margin: 0 auto;
}

.access-id-display {
  margin-top: 10px;
  padding: 10px;
  background-color: #f0f8ff;
  border-radius: 4px;
  font-family: monospace;
}

.progress-container {
  margin-top: 10px;
}

.status-info {
  margin-top: 10px;
  padding: 10px;
  border-radius: 4px;
  background-color: #f5f5f5;
}

.status-info .success {
  color: #52c41a;
}
</style>