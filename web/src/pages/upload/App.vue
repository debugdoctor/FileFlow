<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { FileText, HardDrive, X, Copy, Check, Files, Square, Settings2, ChevronDown } from 'lucide-vue-next';
import { message, Button, Upload, Progress, Typography, Space, InputNumber, Select } from 'ant-design-vue';
import type { UploadProps } from 'ant-design-vue';
import { uploadFile, fetchJsonWithRetry, fetchWithRetry, getTransferConfig } from '@/utils/requests';
import { processUploadWithConcurrencyLimit } from '@/utils/asyncPool';
import JSZip from 'jszip';
import { useTransferRate, formatRate } from '@/utils/useTransferRate';
import { waitWithSignal } from '@/utils/http';

let chunkSize = 1024 * 1024;
let maxTotalSize = chunkSize * 10240;
const P2P_CHUNK_SIZE = 16 * 1024;
const P2P_MAX_BUFFERED_AMOUNT = 8 * 1024 * 1024;
const P2P_CONNECT_TIMEOUT_MS = 15000;
const P2P_SIGNAL_POLL_MS = 1000;

const fileList = ref<UploadProps['fileList']>([]);
const uploadState = ref<'idle' | 'pending' | 'processing' | 'finished'>('idle');
const isStopping = ref(false);
let uploadController: AbortController | null = null;
let completionTimer: ReturnType<typeof setTimeout> | undefined;
const stopUpload = () => {
  if (!uploadController || isStopping.value) return;
  isStopping.value = true;
  uploadController.abort(new DOMException('上传已停止', 'AbortError'));
  speed.stop();
};
const isFolderUpload = ref(false);
const zipFile = ref<File | null>(null);

const accessId = ref<string | null>(null);
const downloadLink = computed(() => accessId.value
  ? new URL(`/${encodeURIComponent(accessId.value)}/file`, window.location.origin).href
  : '');
const linkCopied = ref(false);
let copyFeedbackTimer: ReturnType<typeof setTimeout> | undefined;
const showCopyFeedback = () => {
  linkCopied.value = true;
  if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  copyFeedbackTimer = setTimeout(() => { linkCopied.value = false; }, 2000);
};

const copyDownloadLink = async () => {
  if (!downloadLink.value) return;
  if (window.isSecureContext && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(downloadLink.value);
      showCopyFeedback();
      return;
    } catch {
      // Fall back to selection-based copying when clipboard permission is denied.
    }
  }
  // Support HTTP deployments where the Clipboard API is unavailable.
  const input = document.createElement('textarea');
  const activeElement = document.activeElement;
  input.value = downloadLink.value;
  input.readOnly = true;
  input.style.position = 'fixed';
  input.style.left = '-9999px';
  document.body.appendChild(input);
  try {
    input.select();
    if (!document.execCommand('copy')) throw new Error('Copy failed');
    showCopyFeedback();
  } catch {
    message.warning('复制失败，请选中下载链接手动复制');
  } finally {
    input.remove();
    if (activeElement instanceof HTMLElement) activeElement.focus();
  }
};
const uploadProgress = ref(0);
const receiverWaitMinutes = ref(5);
const selectedChunkSize = ref(1024 * 1024);
const uploadConcurrency = ref(4);
const serverChunkLimit = ref(1024 * 1024);
const chunkOptions = computed(() => {
  const sizes = [...new Set([64, 128, 256, 512, 1024].map(size => size * 1024).filter(size => size <= serverChunkLimit.value).concat(serverChunkLimit.value))].sort((a, b) => a - b);
  return sizes.map(value => ({ value, label: `${formatBytes(value)}${value === serverChunkLimit.value ? '（默认）' : ''}` }));
});
const uploadedLength = ref(0);
const speed = useTransferRate(() => uploadedLength.value);
const is_online = ref(false);
const remainingPolls = ref(5 * 60);

const intervalRef = ref<ReturnType<typeof setInterval> | undefined>(undefined)
const uploadRef = ref<any>(null)
const folderInputRef = ref<HTMLInputElement | null>(null);

const { Title, Text } = Typography;

interface UploadInfo {
  is_final: number;
  filename: string;
  start: number;
  end: number;
  total: number;
}

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

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

const getP2pConfig = async (): Promise<P2pConfig> => {
  try {
    const { data, response } = await fetchJsonWithRetry<{ success?: boolean; data?: P2pConfig }>(
      '/api/fileflow/p2p-config',
      { method: 'get', signal: uploadController?.signal },
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

const getRelativePath = (file: any) => (file?.originFileObj?.webkitRelativePath || file?.webkitRelativePath || '') as string;

const dedupeFiles = (files: UploadProps['fileList'] = []) => {
  const seen = new Set<string>();
  const cleaned: UploadProps['fileList'] = [];

  files.forEach((file) => {
    const origin = (file as any).originFileObj || file;
    const relPath = getRelativePath(file);
    const key = `${relPath}::${origin?.name || file?.name || ''}::${origin?.size || file?.size || 0}`;
    if (!seen.has(key)) {
      seen.add(key);
      cleaned.push(file);
    }
  });

  return cleaned;
};

const isFolderLike = (files: UploadProps['fileList'] = []) =>
  files.some((file) => {
    const relPath = getRelativePath(file);
    return relPath && relPath.split('/').filter(Boolean).length > 1;
  });

const addFileToList = (file: any) => {
  const merged = [...(fileList.value || []), file];
  fileList.value = dedupeFiles(merged);
  const listLen = fileList.value?.length || 0;
  isFolderUpload.value = listLen > 1 || isFolderLike(fileList.value);
};

const triggerFolderSelect = () => {
  if (uploadState.value !== 'idle') return;
  folderInputRef.value?.click();
};

const handleFolderSelect = (event: Event) => {
  const target = event.target as HTMLInputElement | null;
  const files = target?.files;
  if (!files || files.length === 0) return;

  Array.from(files).forEach((file) => addFileToList(file));
  // Reset input so selecting the same folder again works
  if (target) {
    target.value = '';
  }
};

const handleRemove: UploadProps['onRemove'] = file => {
  if (uploadState.value !== 'idle') return false;
  const index = fileList.value!.indexOf(file);
  const newFiles = fileList.value!.slice();
  newFiles.splice(index, 1);
  fileList.value = newFiles;
  isFolderUpload.value = newFiles.length > 1 || isFolderLike(newFiles);
};

const beforeUpload: UploadProps['beforeUpload'] = (file) => {
  // Handle both files and directories
  if (file) {
    addFileToList(file);
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
    const zipBlob = await zip.generateAsync({ type: 'blob' }, () => uploadController?.signal.throwIfAborted());
    const zipFileName = `${folderName}.zip`;
    return new File([zipBlob], zipFileName, { type: 'application/zip' });
  } catch (error) {
    if (uploadController?.signal.aborted) throw error;
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
    const cleanedList = dedupeFiles(newFileList);
    fileList.value = cleanedList;

    const hasMultipleFiles = cleanedList.length > 1;
    // Treat as folder upload when multiple files or any relative path exists
    isFolderUpload.value = hasMultipleFiles || isFolderLike(cleanedList);
  }
};

type FileSystemFileEntry = FileSystemEntry & {
  file: (callback: (file: File) => void) => void;
  fullPath: string;
};

type FileSystemDirectoryReader = {
  readEntries: (cb: (entries: FileSystemEntry[]) => void) => void;
};

type FileSystemDirectoryEntry = FileSystemEntry & {
  createReader: () => FileSystemDirectoryReader;
};

const handleDrop = (e: DragEvent) => {
  e.preventDefault();
  e.stopPropagation();
  // Handle drop event for folder structure
  const items = e.dataTransfer?.items;
  if (items) {
    let hasDirectory = false;
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.kind === 'file') {
        const entry = item.webkitGetAsEntry();
        if (entry) {
          if (entry.isDirectory) {
            hasDirectory = true;
            processEntry(entry);
          } else {
            const fileEntry = entry as FileSystemFileEntry;
            if (typeof fileEntry.file === 'function') {
              fileEntry.file((file: File) => addFileToList(file));
            }
          }
        }
      }
    }
  }
};

const processEntry = (entry: FileSystemEntry) => {
  if (entry.isFile) {
    const fileEntry = entry as FileSystemFileEntry;
    if (typeof fileEntry.file === 'function') {
      fileEntry.file((file: File) => {
        const cleanPath = fileEntry.fullPath.replace(/^\//, '');
        const pathParts = cleanPath.split('/').filter(Boolean);
        // Only keep relative path when a real folder structure exists.
        const fileWithPath = pathParts.length > 1
          ? Object.defineProperty(file, 'webkitRelativePath', { value: cleanPath })
          : file;
        addFileToList(fileWithPath as any);
      });
    }
  } else if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    const readBatch = () => reader.readEntries((entries: FileSystemEntry[]) => {
      if (entries.length === 0) return;
      entries.forEach(processEntry);
      readBatch();
    });
    readBatch();
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
    uploadController?.signal.throwIfAborted();
    const config = await getTransferConfig(uploadController?.signal);
    serverChunkLimit.value = config.max_chunk_size || config.chunk_size;
    selectedChunkSize.value = Math.max(config.min_chunk_size || Math.min(64 * 1024, serverChunkLimit.value), Math.min(selectedChunkSize.value, serverChunkLimit.value));
    chunkSize = selectedChunkSize.value;
    maxTotalSize = config.max_file_size;
    const fileToUpload = (isFolderUpload.value ? zipFile.value : fileList.value?.[0]) as File | undefined;
    if (!fileToUpload) {
      message.error('没有可上传的文件');
      return;
    }

    const fileSize = fileToUpload.size ?? 0;
    if (fileSize === 0) {
      message.error('无法获取文件大小');
      return;
    }

    if (fileSize > maxTotalSize) {
      message.error(`文件过大，单次上传上限为 ${formatBytes(maxTotalSize)}`);
      return;
    }
    const { data, response } = await fetchJsonWithRetry<{ success?: boolean; data?: { id: string } }>(
      `/api/fileflow/id?${new URLSearchParams({ file_name: fileToUpload.name, file_size: String(fileSize), chunk_size: String(chunkSize) })}`,
      { method: 'get', signal: uploadController?.signal },
      { timeoutMs: 12000, retries: 2 },
    );

    if (!response.ok) {
      const msg = (data as any)?.message || '未能生成有效的下载链接';
      throw new Error(msg);
    }

    if (!data?.data?.id) {
      throw new Error('未能生成有效的下载链接');
    }

    accessId.value = data.data.id;
    message.success('下载链接已生成，请发送给接收方');
    return accessId.value;
  } catch (error) {
    if (uploadController?.signal.aborted) throw error;
    const msg = error instanceof Error ? error.message : '未知错误';
    message.error(`生成下载链接失败: ${msg}`);
    throw error;
  }
};

const postSignal = async (payload: { role: 'sender' | 'receiver'; type: string; data: any; rid?: string }) => {
  if (!accessId.value) {
    throw new Error('ID 为空，无法发送信令');
  }
  const response = await fetchWithRetry(
    `/api/fileflow/${accessId.value}/signal`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
      signal: uploadController?.signal,
    },
    { timeoutMs: 6000, retries: 2 },
  );
  if (!response.ok) throw new Error(`信令发送失败: ${response.status}`);
};

const getSignals = async (role: 'sender' | 'receiver', since: number) => {
  if (!accessId.value) {
    throw new Error('ID 为空，无法获取信令');
  }
  const { data } = await fetchJsonWithRetry<{ success?: boolean; data?: { latest?: number; messages?: SignalMessage[] } }>(
    `/api/fileflow/${accessId.value}/signal?role=${role}&since=${since}`,
    { method: 'get', signal: uploadController?.signal },
    { timeoutMs: 6000, retries: 2 },
  );
  return data?.data || { latest: since, messages: [] };
};

const resetUpload = (by_error: boolean) => {
  isStopping.value = false;
  if (completionTimer) clearTimeout(completionTimer);
  linkCopied.value = false;
  if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  speed.stop();
  // Reset all states after upload completion
  remainingPolls.value = 0;
  uploadProgress.value = 0;
  uploadedLength.value = 0;
  uploadState.value = 'idle';
  accessId.value = null;
  zipFile.value = null;
  isFolderUpload.value = by_error && ((fileList.value?.length || 0) > 1 || isFolderLike(fileList.value));
  if (!by_error) {
    fileList.value = [];
  }
};

const uploadViaP2P = async (nativeFile: File, config: P2pConfig) => {
  const iceServers = buildIceServers(config);
  if (iceServers.length === 0) {
    return false;
  }

  let signalSeq = 0;
  let pollActive = true;
  let channelOpen = false;
  let shouldCleanup = false;
  let transferError: Error | null = null;
  let acknowledged = false;
  const pendingCandidates: RTCIceCandidateInit[] = [];

  const pc = new RTCPeerConnection({ iceServers });
  const channel = pc.createDataChannel('file', { ordered: true });
  channel.binaryType = 'arraybuffer';
  channel.bufferedAmountLowThreshold = Math.max(64 * 1024, P2P_MAX_BUFFERED_AMOUNT / 2);

  const cleanup = () => {
    pollActive = false;
    if (channel.readyState !== 'closed') {
      channel.close();
    }
    pc.close();
  };
  const signal = uploadController?.signal;
  const cancelP2P = () => {
    transferError = new DOMException('上传已停止', 'AbortError');
    channel.dispatchEvent(new Event('error'));
    cleanup();
  };
  signal?.addEventListener('abort', cancelP2P, { once: true });

  const waitForChannelOpen = () =>
    new Promise<void>((resolve, reject) => {
      if (signal?.aborted || transferError) { reject(transferError || signal?.reason); return; }
      if (channel.readyState === 'open') {
        channelOpen = true;
        resolve();
        return;
      }
      const timer = setTimeout(() => {
        channel.removeEventListener('open', handleOpen);
        channel.removeEventListener('error', handleError);
        reject(new Error('P2P 连接超时'));
      }, P2P_CONNECT_TIMEOUT_MS);

      const handleOpen = () => {
        channelOpen = true;
        clearTimeout(timer);
        channel.removeEventListener('open', handleOpen);
        channel.removeEventListener('error', handleError);
        resolve();
      };
      const handleError = () => {
        clearTimeout(timer);
        channel.removeEventListener('open', handleOpen);
        channel.removeEventListener('error', handleError);
        reject(new Error('P2P 连接失败'));
      };

      channel.addEventListener('open', handleOpen);
      channel.addEventListener('error', handleError);
    });

  const waitForBufferLow = async () => {
    const deadline = Date.now() + 30000;
    while (channel.bufferedAmount >= P2P_MAX_BUFFERED_AMOUNT) {
      if (transferError) throw transferError;
      if (channel.readyState !== 'open' || Date.now() >= deadline) {
        throw new Error('P2P 发送中断或超时');
      }
      await sleep(50);
    }
    if (transferError) throw transferError;
  };

  const handleSignal = async (msg: SignalMessage) => {
    if (msg.msg_type === 'answer' && msg.data) {
      const desc = new RTCSessionDescription(msg.data);
      if (!pc.currentRemoteDescription) {
        await pc.setRemoteDescription(desc);
        for (const candidate of pendingCandidates.splice(0)) {
          await pc.addIceCandidate(candidate);
        }
      }
    } else if (msg.msg_type === 'candidate' && msg.data) {
      try {
        if (!pc.remoteDescription) pendingCandidates.push(msg.data);
        else await pc.addIceCandidate(new RTCIceCandidate(msg.data));
      } catch (error) {
        console.warn('添加 ICE 候选失败', error);
      }
    } else if (msg.msg_type === 'fallback') {
      throw new Error('接收方要求回退 HTTP');
    }
  };

  pc.onicecandidate = (event) => {
    if (event.candidate) {
      void postSignal({ role: 'sender', type: 'candidate', data: event.candidate }).catch(error => {
        transferError = error instanceof Error ? error : new Error('ICE 信令发送失败');
        channel.dispatchEvent(new Event('error'));
      });
    }
  };

  pc.onconnectionstatechange = () => {
    if (['failed', 'disconnected', 'closed'].includes(pc.connectionState)) {
      transferError = new Error('P2P 连接中断');
      if (!channelOpen) {
        channel.dispatchEvent(new Event('error'));
      }
    }
  };

  channel.addEventListener('message', (event) => {
    if (typeof event.data === 'string') {
      try {
        const msg = JSON.parse(event.data);
        if (msg?.type === 'done') {
          acknowledged = true;
          message.success('接收方已完成 P2P 下载');
        }
      } catch {
        return;
      }
    }
  });

  const pollSignals = async () => {
    while (pollActive) {
      try {
        const signalData = await getSignals('sender', signalSeq);
        for (const msg of signalData.messages || []) {
          if (msg.msg_type === 'fallback') {
            transferError = new Error('接收方要求回退 HTTP');
            channel.dispatchEvent(new Event('error'));
            pollActive = false;
            break;
          }
          await handleSignal(msg);
        }
        signalSeq = signalData.latest ?? signalSeq;
      } catch (error) {
        console.warn('获取信令失败', error);
      }
      await sleep(P2P_SIGNAL_POLL_MS);
    }
  };

  try {
    void pollSignals();
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    await postSignal({ role: 'sender', type: 'offer', data: offer });

    await waitForChannelOpen();

    channel.send(JSON.stringify({ type: 'meta', name: nativeFile.name, size: nativeFile.size }));

    const totalSize = nativeFile.size;
    let offset = 0;
    while (offset < totalSize) {
      signal?.throwIfAborted();
      const chunk = nativeFile.slice(offset, offset + P2P_CHUNK_SIZE);
      const buffer = await chunk.arrayBuffer();
      channel.send(buffer);
      offset += chunk.size;
      uploadedLength.value = offset;
      uploadProgress.value = Math.round((uploadedLength.value / totalSize) * 100);
      await waitForBufferLow();
    }

    channel.send(JSON.stringify({ type: 'end' }));
    const deadline = Date.now() + 30000;
    while (!acknowledged) {
      if (transferError) throw transferError;
      if (channel.readyState !== 'open' || Date.now() >= deadline) {
        throw new Error('未收到 P2P 下载完成确认');
      }
      await sleep(100);
    }
    cleanup();
    return true;
  } catch (error) {
    cleanup();
    try {
      await postSignal({ role: 'sender', type: 'fallback', data: { message: error instanceof Error ? error.message : 'P2P 失败' } });
    } catch {
      // ignore signaling errors during fallback
    }
    shouldCleanup = true;
    throw error;
  } finally {
    signal?.removeEventListener('abort', cancelP2P);
    if (shouldCleanup) {
      cleanup();
    }
  }
};

const handleUpload = async () => {
  if (uploadState.value !== 'idle') return;
  const controller = new AbortController();
  uploadController = controller;
  isStopping.value = false;
  uploadState.value = 'pending';
  // If we don't have an accessId yet, get one
  if (!accessId.value) {
    try {
      await getAccessId();
    } catch (error) {
      if (controller.signal.aborted) message.info('上传已停止');
      resetUpload(true);
      return;
    }
  }
  if (!accessId.value) {
    resetUpload(true);
    return;
  }

  uploadState.value = 'pending';
  uploadProgress.value = 0;

  message.info('正在等待接收方连接...');
  const waitMinutes = Math.min(1440, Math.max(1, Math.round(Number(receiverWaitMinutes.value) || 5)));
  receiverWaitMinutes.value = waitMinutes;
  const deadline = Date.now() + waitMinutes * 60000;
  remainingPolls.value = waitMinutes * 60;

  try {
    // Polling to check if the access code is in use
    let isUsing = false;
    const countdown = setInterval(() => {
      remainingPolls.value = Math.max(0, Math.ceil((deadline - Date.now()) / 1000));
    }, 250);
    try {
      while (!isUsing && Date.now() < deadline) {
        controller.signal.throwIfAborted();
        try {
          const { data: statusData } = await fetchJsonWithRetry<{ success?: boolean; data?: { is_using?: boolean } }>(
            `/api/fileflow/${accessId.value}/status`,
            { method: 'get', signal: controller.signal },
            { timeoutMs: Math.min(6000, Math.max(1, deadline - Date.now())), retries: 0 },
          );
          if (statusData?.success && statusData.data?.is_using) {
            isUsing = true;
            uploadState.value = 'processing';
            message.success('接收方已连接，开始上传文件...');
          }
        } catch {
          controller.signal.throwIfAborted();
          // A brief network interruption still counts toward the configured deadline.
        }
        if (!isUsing) await waitWithSignal(Math.min(1000, Math.max(0, deadline - Date.now())), controller.signal);
      }
    } finally {
      clearInterval(countdown);
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
      const chunks = Math.ceil(fileSize / chunkSize);

      message.info(`开始上传${isFolderUpload.value ? '压缩' : ''}文件: ${fileName} (${formatBytes(fileSize)})`);

      // Create an array to hold all chunk upload promises
      const uploadPromises: Array<() => Promise<number>> = [];

      // Create all chunk upload functions
      for (let i = 0; i < chunks; i++) {
        const start = i * chunkSize;
        const end = Math.min(start + chunkSize, fileSize);
        const chunkIndex = i; // 保存当前索引的快照

        // Allocate multipart data only when a worker can actually send it.
        uploadPromises.push(() => uploadFile(() => {
          const chunk = nativeFile.slice(start, end);
          const formData = new FormData();
          formData.append('info', JSON.stringify({
            filename: fileName,
            start: start,
            end: end - 1,
            total: fileSize,
          } as UploadInfo));
          formData.append('file', chunk);
          return formData;
        }, accessId.value, chunkIndex, end - start, start, controller.signal));
      }

      // Process uploads with a concurrency limit of 4
      await processUploadWithConcurrencyLimit(uploadPromises, Math.max(1, Math.min(8, Math.round(uploadConcurrency.value))), (seq_len) => {
        controller.signal.throwIfAborted();
        uploadedLength.value += seq_len;
        uploadProgress.value = Math.round((uploadedLength.value / fileSize) * 100);
      });
    };

    if (!accessId.value) {
      message.error('ID 为空，无法建立连接');
      resetUpload(true);
      return;
    }

    uploadedLength.value = 0;
    uploadProgress.value = 0;
    speed.start();

    const p2pConfig = await getP2pConfig();
    controller.signal.throwIfAborted();
    if (hasP2pConfig(p2pConfig)) {
      try {
        message.warning('正在尝试 P2P 连接...');
        await uploadViaP2P(nativeFile, p2pConfig);
      } catch (error) {
        controller.signal.throwIfAborted();
        message.warning('P2P 传输失败，已回退到 HTTP');
        uploadedLength.value = 0;
        uploadProgress.value = 0;
        speed.start();
        await uploadViaHttp();
      }
    } else {
      await uploadViaHttp();
    }
    controller.signal.throwIfAborted();
    uploadState.value = 'finished';
    speed.stop();
    message.success('文件上传成功！等待接收方下载完成...');

    let retry: number = 0;
    // Start polling to check if download is complete
    const checkDownloadComplete = async () => {
      if (controller.signal.aborted || uploadController !== controller) return;
      retry++;
      if (retry > 20) {
        message.error('未能得知文件是否被下载完成，确认下载完成则可以关闭窗口！');
        return;
      }

      try {
        const { data } = await fetchJsonWithRetry<{ success?: boolean; data?: { done?: boolean } }>(
          `/api/fileflow/${accessId.value}/status`,
          { method: 'get', signal: controller.signal },
          { timeoutMs: 6000, retries: 2 },
        );

        if (data?.success && data.data && data.data.done) {
          message.success('接收方已下载完成！');
          // Reset everything after a short delay
          completionTimer = setTimeout(() => {
            resetUpload(false);
          }, 1500);
        } else {
          // Continue polling every second
          completionTimer = setTimeout(checkDownloadComplete, 1000);
        }
      } catch (error) {
        if (controller.signal.aborted) return;
        message.warning('检查下载状态时出错，但将继续尝试');
        // Continue polling even on error
        completionTimer = setTimeout(checkDownloadComplete, 1000);
      }
    };

    // Start polling after a short delay
    completionTimer = setTimeout(checkDownloadComplete, 1000);
  } catch (error) {
    resetUpload(true);
    if (controller.signal.aborted) { message.info('上传已停止'); return; }
    const errorMessage = error instanceof Error ? error.message : '未知错误';
    message.error("上传失败: " + errorMessage);
  }
};


const sayHello = async () => {
  try {
    const response = await fetchWithRetry(
      '/api/fileflow/hello',
      { method: 'get' },
      { timeoutMs: 3000, retries: 1 },
    );
    is_online.value = response.ok;
  } catch {
    is_online.value = false;
  }
};

onMounted(() => {
  void getTransferConfig().then(config => {
    if (uploadState.value !== 'idle') return;
    serverChunkLimit.value = config.max_chunk_size || config.chunk_size;
    selectedChunkSize.value = Math.min(selectedChunkSize.value, serverChunkLimit.value);
  }).catch(() => {});
  sayHello();
  intervalRef.value = setInterval(() => {
    sayHello();
  }, 5000);
});

onUnmounted(() => {
  uploadController?.abort();
  if (completionTimer) clearTimeout(completionTimer);
  if (copyFeedbackTimer) clearTimeout(copyFeedbackTimer);
  if (intervalRef.value) {
    clearInterval(intervalRef.value);
  }
});
</script>

<template>
  <nav class="site-nav"><a href="/" class="brand"><span class="brand-mark"><Files :size="23" :stroke-width="1.7" /></span>FileFlow</a><a href="/" class="nav-back">返回首页</a></nav>
  <div class="file-upload-container">
    <div class="upload-card">
      <Space direction="vertical" size="large" style="width: 100%; max-width: 600px;">
        <div class="connection-status" role="status" aria-live="polite"><span class="connection-dot" :class="{ online: is_online }"></span><span>{{ is_online ? '服务器已连接' : '服务器未连接' }}</span></div>

        <div v-if="uploadState === 'pending'" class="wait-time-container">
          <Text type="warning">等待接收方连接中... 剩余等待时间: {{ remainingPolls }} 秒</Text>
        </div>

        <div class="file-upload-area">
          <Upload.Dragger v-if="uploadState === 'idle'" ref="uploadRef" :file-list="fileList" :before-upload="beforeUpload" @remove="handleRemove"
            :disabled="uploadState !== 'idle'" :multiple="true" name="file" :show-upload-list="false"
            @change="handleChange" @drop="handleDrop">
            <div class="upload-area-wrapper">
              <p class="ant-upload-text">点击或拖拽文件/文件夹到此区域上传</p>
              <p class="ant-upload-hint">
                支持文件或文件夹上传；多个文件或文件夹将自动打包。请勿上传敏感数据。
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

        <div class="download-link-section">
          <div v-if="downloadLink" class="download-link-display">
            <Text strong>下载链接:</Text>
            <div class="link-container">
              <a :href="downloadLink" :title="downloadLink" target="_blank" rel="noopener noreferrer">{{ downloadLink }}</a>
              <button type="button" class="copy-link-button" :class="{ copied: linkCopied }"
                :aria-label="linkCopied ? '链接已复制' : '复制链接'" :title="linkCopied ? '已复制' : '复制链接'"
                @click="copyDownloadLink">
                <Check v-if="linkCopied" :size="18" :stroke-width="1.8" />
                <Copy v-else :size="18" :stroke-width="1.8" />
              </button>
            </div>
            <span class="visually-hidden" role="status">{{ linkCopied ? '下载链接已复制' : '' }}</span>
            <p><Text type="secondary">将链接发送给接收方，打开即可进入下载页</Text></p>
          </div>
        </div>

        <details class="advanced-settings">
          <summary><Settings2 :size="16" :stroke-width="1.7" /><span>高级设置</span><ChevronDown :size="15" class="advanced-chevron" /></summary>
          <div class="advanced-fields">
            <div class="advanced-field"><label for="upload-chunk-size">分块大小</label>
              <Select id="upload-chunk-size" aria-label="分块大小" v-model:value="selectedChunkSize" :options="chunkOptions" :disabled="uploadState !== 'idle'" />
            </div>
            <div class="advanced-field"><label for="upload-concurrency">上传并发</label>
              <Select id="upload-concurrency" aria-label="上传并发" v-model:value="uploadConcurrency" :options="[1, 2, 4, 8].map(value => ({ value, label: `${value} 路${value === 4 ? '（默认）' : ''}` }))" :disabled="uploadState !== 'idle'" />
            </div>
            <div class="advanced-field"><label for="receiver-wait-minutes">接收等待</label>
              <InputNumber id="receiver-wait-minutes" v-model:value="receiverWaitMinutes" aria-label="等待接收方连接时间（分钟）" :min="1" :max="1440" :precision="0" addon-after="分钟" :disabled="uploadState !== 'idle'" />
            </div>
          </div>
          <p class="advanced-hint">默认即可使用。网络波动时，可减小分块或降低并发。</p>
        </details>

        <div class="action-buttons">
          <Button type="default" size="large" :disabled="uploadState !== 'idle'" @click="triggerFolderSelect">
            选择文件夹
          </Button>
          <Button type="primary" size="large" :disabled="!fileList || fileList.length === 0 || uploadState !== 'idle'"
            :loading="uploadState === 'pending' || uploadState === 'processing'" @click="handleUpload"
            class="upload-button">
          <template #icon>
            <HardDrive />
          </template>
          {{ uploadState === 'processing' ? '上传中...' : uploadState === 'pending' ? '等待对方接收' : '生成链接并上传' }}
          </Button>
          <Button v-if="uploadState === 'pending' || uploadState === 'processing'" size="large" class="stop-transfer-button"
            :loading="isStopping" :disabled="isStopping" aria-label="停止上传" title="停止上传" @click="stopUpload">
            <template #icon><Square :size="18" :stroke-width="1.8" /></template>
          </Button>
        </div>

        <div v-if="uploadState === 'processing'" class="progress-container">
          <Progress :percent="uploadProgress" size="small" />
          <div class="transfer-metrics"><span>{{ formatBytes(uploadedLength) }} 已发送</span><strong>{{ formatRate(speed.rate.value) }}</strong></div>
        </div>

        <div class="instructions">
          <Title :level="5">使用说明:</Title>
          <ul>
            <li>选择或拖拽文件/文件夹；多个文件会自动打包</li>
            <li>点击“生成链接并上传”，复制下载链接发给接收方</li>
            <li>接收方打开链接后点击“开始下载”，过程中请保持此页开启</li>
          </ul>
        </div>
      </Space>
      <input
        ref="folderInputRef"
        type="file"
        webkitdirectory
        style="display: none"
        @change="handleFolderSelect"
      />
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

.download-link-section {
  width: 100%;
  text-align: center;
}

.download-link-display {
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

.action-buttons {
  display: flex;
  width: 100%;
  gap: 12px;
  margin: 8px 0;
}

.upload-button {
  flex: 1;
  height: 48px;
  font-size: 16px;
  display: flex;
  justify-content: center;
  align-items: center;
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
