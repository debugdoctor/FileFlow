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

const P2P_MAX_BUFFERED_AMOUNT = 8 * 1024 * 1024;
const P2P_CONNECT_TIMEOUT_MS = 5000;
const P2P_SIGNAL_POLL_MS = 1000;

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

type P2pConfig = {
  stun?: string;
  turn?: string;
  turn_username?: string;
  turn_credential?: string;
};

type SignalMessage = {
  seq: number;
  from: string;
  msg_type: string;
  data: any;
  rid?: string;
};

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

const getP2pConfig = async (): Promise<P2pConfig> => {
  try {
    const { data, response } = await fetchJsonWithRetry<{ success?: boolean; data?: P2pConfig }>(
      '/api/fileflow/p2p-config',
      { method: 'get' },
      { timeoutMs: 6000, retries: 1 },
    );
    if (response.ok && data?.success) {
      return data.data || {};
    }
  } catch {
    return {};
  }
  return {};
};

const hasP2pConfig = (config: P2pConfig) => {
  return !!(config.stun || config.turn);
};

const buildIceServers = (config: P2pConfig): RTCIceServer[] => {
  const servers: RTCIceServer[] = [];
  if (config.stun) {
    servers.push({ urls: [config.stun] });
  }
  if (config.turn) {
    const turnServer: RTCIceServer = { urls: [config.turn] };
    if (config.turn_username && config.turn_credential) {
      turnServer.username = config.turn_username;
      turnServer.credential = config.turn_credential;
    }
    servers.push(turnServer);
  }
  return servers;
};

const postSignal = async (payload: { role: 'sender' | 'receiver'; type: string; data: any; rid?: string }) => {
  if (!activeFileId.value) {
    throw new Error('AccessId 为空，无法发送信令');
  }
  await fetchWithRetry(
    `/api/fileflow/${activeFileId.value}/signal`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    },
    { timeoutMs: 6000, retries: 2 },
  );
};

const getSignals = async (role: 'sender' | 'receiver', since: number) => {
  if (!activeFileId.value) {
    throw new Error('AccessId 为空，无法获取信令');
  }
  const { data } = await fetchJsonWithRetry<{ success?: boolean; data?: { latest?: number; messages?: SignalMessage[] } }>(
    `/api/fileflow/${activeFileId.value}/signal?role=${role}&since=${since}`,
    { method: 'get' },
    { timeoutMs: 6000, retries: 2 },
  );
  return data?.data || { latest: since, messages: [] };
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

const downloadViaP2P = async (config: P2pConfig) => {
  const iceServers = buildIceServers(config);
  if (iceServers.length === 0) {
    throw new Error('P2P 配置为空');
  }

  const receiverId = localStorage.getItem('rid') || '';
  let signalSeq = 0;
  let pollActive = true;
  let completed = false;
  let shouldCleanup = false;
  let transferError: Error | null = null;

  let dataChannel: RTCDataChannel | null = null;
  const pc = new RTCPeerConnection({ iceServers });

  let incomingName = fileName.value || 'downloaded_file';
  let totalSize = fileSize.value || 0;
  let receivedBytes = 0;
  const chunks: ArrayBuffer[] = [];

  const cleanup = () => {
    pollActive = false;
    if (dataChannel && dataChannel.readyState !== 'closed') {
      dataChannel.close();
    }
    pc.close();
  };

  const finalizeDownload = async () => {
    if (completed) return;
    completed = true;

    const blob = new Blob(chunks, { type: 'application/octet-stream' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = incomingName || 'downloaded_file';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    message.success('文件下载完成!');

    try {
      await fetchWithRetry(
        `/api/fileflow/${activeFileId.value}/done`,
        {
          method: 'PUT',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({}),
        },
        { timeoutMs: 6000, retries: 2 },
      );
    } catch {
      message.warning('无法通知服务器下载完成，但文件已成功下载');
    }

    if (dataChannel?.readyState === 'open') {
      dataChannel.send(JSON.stringify({ type: 'done' }));
    }

    if (totalSize > 0) {
      downloadProgress.value = 100;
    }
  };

  const handleSignal = async (msg: SignalMessage) => {
    if (msg.msg_type === 'offer' && msg.data) {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.data));
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      await postSignal({ role: 'receiver', type: 'answer', data: answer, rid: receiverId });
    } else if (msg.msg_type === 'candidate' && msg.data) {
      try {
        await pc.addIceCandidate(new RTCIceCandidate(msg.data));
      } catch (error) {
        console.warn('添加 ICE 候选失败', error);
      }
    } else if (msg.msg_type === 'fallback') {
      throw new Error('发送方要求回退 HTTP');
    }
  };

  pc.onicecandidate = (event) => {
    if (event.candidate) {
      void postSignal({ role: 'receiver', type: 'candidate', data: event.candidate, rid: receiverId });
    }
  };

  pc.onconnectionstatechange = () => {
    if (['failed', 'disconnected', 'closed'].includes(pc.connectionState) && !completed) {
      transferError = new Error('P2P 连接中断');
      completed = true;
    }
  };

  const channelReady = new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('P2P 连接超时')), P2P_CONNECT_TIMEOUT_MS);
    pc.ondatachannel = (event) => {
      dataChannel = event.channel;
      dataChannel.binaryType = 'arraybuffer';
      dataChannel.bufferedAmountLowThreshold = Math.max(64 * 1024, P2P_MAX_BUFFERED_AMOUNT / 2);

      const handleOpen = () => {
        clearTimeout(timer);
        resolve();
      };
      const handleError = () => {
        clearTimeout(timer);
        reject(new Error('P2P 连接失败'));
      };

      dataChannel.addEventListener('open', handleOpen);
      dataChannel.addEventListener('error', handleError);
      dataChannel.addEventListener('close', () => {
        if (!completed) {
          transferError = new Error('P2P 连接关闭');
          completed = true;
        }
      });
      dataChannel.addEventListener('message', async (event) => {
        if (typeof event.data === 'string') {
          try {
            const msg = JSON.parse(event.data);
            if (msg?.type === 'meta') {
              if (msg.name) {
                incomingName = msg.name;
                fileName.value = msg.name;
              }
              if (msg.size) {
                totalSize = msg.size;
                fileSize.value = msg.size;
              }
              return;
            }
            if (msg?.type === 'end') {
              await finalizeDownload();
              return;
            }
          } catch {
            return;
          }
        }

        if (event.data instanceof ArrayBuffer) {
          chunks.push(event.data);
          receivedBytes += event.data.byteLength;
          if (totalSize > 0) {
            downloadProgress.value = Math.round((receivedBytes / totalSize) * 100);
            if (receivedBytes >= totalSize) {
              await finalizeDownload();
            }
          }
        }
      });
    };
  });

  const pollSignals = async () => {
    while (pollActive) {
      try {
        const signalData = await getSignals('receiver', signalSeq);
        signalSeq = signalData.latest ?? signalSeq;
        for (const msg of signalData.messages || []) {
          await handleSignal(msg);
        }
      } catch (error) {
        console.warn('获取信令失败', error);
      }
      await sleep(P2P_SIGNAL_POLL_MS);
    }
  };

  try {
    await postSignal({ role: 'receiver', type: 'ready', data: {}, rid: receiverId });
    void pollSignals();
    await channelReady;

    while (!completed) {
      await sleep(500);
    }
    pollActive = false;
    if (transferError) {
      throw transferError;
    }
    cleanup();
    return true;
  } catch (error) {
    shouldCleanup = true;
    try {
      await postSignal({
        role: 'receiver',
        type: 'fallback',
        data: { message: error instanceof Error ? error.message : 'P2P 失败' },
        rid: receiverId,
      });
    } catch {
      // ignore signaling errors during fallback
    }
    throw error;
  } finally {
    if (shouldCleanup) {
      cleanup();
    }
  }
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

  const p2pConfig = await getP2pConfig();
  if (hasP2pConfig(p2pConfig)) {
    try {
      message.warning('正在尝试 P2P 连接...');
      await downloadViaP2P(p2pConfig);
      isDownloading.value = false;
      isFinished.value = true;
      return;
    } catch (error) {
      message.warning('P2P 传输失败，已回退到 HTTP');
      downloadProgress.value = 0;
      isFinished.value = false;
    }
  }

  await downloadViaHttp();
}

onMounted(async () => {
  if (localStorage.getItem("rid") == null || localStorage.getItem("rid") == "" || localStorage.getItem("rid") == undefined) {
    localStorage.setItem("rid", Math.random().toString(36).slice(2, 10));
  }

  // Get the id from route path name
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
            <li>确保分享 ID 来自可信来源</li>
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
