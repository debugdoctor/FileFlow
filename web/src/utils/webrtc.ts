type SignalRole = 'sender' | 'receiver';

type SignalMessage =
  | { type: 'ready' }
  | { type: 'offer'; sdp: RTCSessionDescriptionInit }
  | { type: 'answer'; sdp: RTCSessionDescriptionInit }
  | { type: 'ice'; candidate: RTCIceCandidateInit }
  | { type: 'fallback_http'; reason?: string }
  | { type: 'p2p_done' }
  | { type: 'error'; message: string };

type TransferStatus = 'success' | 'fallback';

interface WebRtcConfig {
  iceServers: Array<{ urls: string[]; username?: string; credential?: string }>;
}

interface TransferCallbacks {
  onProgress?: (percent: number, loaded: number, total: number) => void;
  onStatus?: (message: string) => void;
  onMetadata?: (name: string, size: number) => void;
}

const DEFAULT_ICE_SERVERS: WebRtcConfig['iceServers'] = [];

const SIGNAL_TIMEOUT_MS = 15000;
const DATA_CHANNEL_NAME = 'fileflow';
const CHUNK_SIZE = 256 * 1024;
const BUFFER_LOW_WATER_MARK = 2 * 1024 * 1024;

const getSignalUrl = (roomId: string, role: SignalRole, rid?: string) => {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const base = `${protocol}://${window.location.host}/api/fileflow/signal/${roomId}`;
  const params = new URLSearchParams({ role });
  if (rid) params.set('rid', rid);
  return `${base}?${params.toString()}`;
};

const safeSend = (ws: WebSocket, payload: SignalMessage) => {
  if (ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
  }
};

const parseSignalMessage = (data: string): SignalMessage | null => {
  try {
    return JSON.parse(data) as SignalMessage;
  } catch {
    return null;
  }
};

const loadWebRtcConfig = async (): Promise<WebRtcConfig> => {
  try {
    const response = await fetch('/api/fileflow/webrtc-config');
    if (!response.ok) {
      return { iceServers: DEFAULT_ICE_SERVERS };
    }
    const data = (await response.json()) as WebRtcConfig;
    if (!data.iceServers || data.iceServers.length === 0) {
      return { iceServers: DEFAULT_ICE_SERVERS };
    }
    return data;
  } catch {
    return { iceServers: DEFAULT_ICE_SERVERS };
  }
};

const setupPeerConnection = (
  config: WebRtcConfig,
  onIceCandidate: (candidate: RTCIceCandidateInit) => void,
) => {
  const pc = new RTCPeerConnection({ iceServers: config.iceServers });
  pc.onicecandidate = event => {
    if (event.candidate) {
      onIceCandidate(event.candidate.toJSON());
    }
  };
  return pc;
};

const downloadBlob = (blob: Blob, filename: string) => {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename || 'downloaded_file';
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
};

const waitForBufferedAmountLow = (channel: RTCDataChannel) =>
  new Promise<void>(resolve => {
    if (channel.bufferedAmount <= BUFFER_LOW_WATER_MARK) {
      resolve();
      return;
    }
    const handler = () => {
      channel.removeEventListener('bufferedamountlow', handler);
      resolve();
    };
    channel.addEventListener('bufferedamountlow', handler);
    channel.bufferedAmountLowThreshold = BUFFER_LOW_WATER_MARK;
  });

const sendFileOverChannel = async (
  file: File,
  channel: RTCDataChannel,
  callbacks?: TransferCallbacks,
) => {
  const meta = {
    type: 'file_meta',
    name: file.name,
    size: file.size,
    chunkSize: CHUNK_SIZE,
  };
  channel.send(JSON.stringify(meta));

  let offset = 0;
  while (offset < file.size) {
    await waitForBufferedAmountLow(channel);
    const slice = file.slice(offset, offset + CHUNK_SIZE);
    const buffer = await slice.arrayBuffer();
    channel.send(buffer);
    offset += buffer.byteLength;
    const percent = Math.round((offset / file.size) * 100);
    callbacks?.onProgress?.(percent, offset, file.size);
  }

  channel.send(JSON.stringify({ type: 'file_end' }));
};

export const sendViaWebRtc = async (
  roomId: string,
  file: File,
  callbacks?: TransferCallbacks,
): Promise<{ status: TransferStatus; reason?: string }> => {
  const config = await loadWebRtcConfig();
  const ws = new WebSocket(getSignalUrl(roomId, 'sender'));

  return new Promise(resolve => {
    let pc: RTCPeerConnection | null = null;
    let dc: RTCDataChannel | null = null;
    let finished = false;
    const timeout = setTimeout(() => {
      fail('timeout', true);
    }, SIGNAL_TIMEOUT_MS);

    const cleanup = () => {
      clearTimeout(timeout);
      dc?.close();
      pc?.close();
      ws.close();
    };

    const finish = (status: TransferStatus, reason?: string) => {
      if (finished) return;
      finished = true;
      cleanup();
      resolve({ status, reason });
    };

    const fail = (reason: string, notifyPeer: boolean) => {
      if (notifyPeer) {
        safeSend(ws, { type: 'fallback_http', reason });
      }
      callbacks?.onStatus?.(`fallback:${reason}`);
      finish('fallback', reason);
    };

    const startOffer = async () => {
      if (pc) return;
      pc = setupPeerConnection(config, candidate => safeSend(ws, { type: 'ice', candidate }));
      pc.onconnectionstatechange = () => {
        if (pc?.connectionState === 'failed') {
          fail('connection_failed', true);
        }
      };
      dc = pc.createDataChannel(DATA_CHANNEL_NAME, { ordered: true });
      dc.binaryType = 'arraybuffer';
      dc.onopen = async () => {
        clearTimeout(timeout);
        try {
          await sendFileOverChannel(file, dc!, callbacks);
          safeSend(ws, { type: 'p2p_done' });
          finish('success');
        } catch (error) {
          fail(error instanceof Error ? error.message : 'send_failed', true);
        }
      };
      dc.onerror = () => fail('datachannel_error', true);

      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      safeSend(ws, { type: 'offer', sdp: offer });
    };

    ws.onmessage = async event => {
      const msg = parseSignalMessage(event.data);
      if (!msg) return;
      if (msg.type === 'ready') {
        await startOffer();
      } else if (msg.type === 'answer' && pc) {
        await pc.setRemoteDescription(msg.sdp);
      } else if (msg.type === 'ice' && pc) {
        await pc.addIceCandidate(msg.candidate);
      } else if (msg.type === 'fallback_http') {
        fail('peer_fallback', false);
      }
    };

    ws.onclose = () => {
      if (!finished) {
        fail('signal_closed', false);
      }
    };
  });
};

export const receiveViaWebRtc = async (
  roomId: string,
  rid: string,
  callbacks?: TransferCallbacks,
): Promise<{ status: TransferStatus; reason?: string }> => {
  const config = await loadWebRtcConfig();
  const ws = new WebSocket(getSignalUrl(roomId, 'receiver', rid));

  return new Promise(resolve => {
    let pc: RTCPeerConnection | null = null;
    let dc: RTCDataChannel | null = null;
    let finished = false;
    let fileName = '';
    let fileSize = 0;
    let receivedBytes = 0;
    let metaReceived = false;
    let endReceived = false;
    const chunks: ArrayBuffer[] = [];
    const timeout = setTimeout(() => {
      fail('timeout', true);
    }, SIGNAL_TIMEOUT_MS);

    const cleanup = () => {
      clearTimeout(timeout);
      dc?.close();
      pc?.close();
      ws.close();
    };

    const finish = (status: TransferStatus, reason?: string) => {
      if (finished) return;
      finished = true;
      cleanup();
      resolve({ status, reason });
    };

    const fail = (reason: string, notifyPeer: boolean) => {
      if (notifyPeer) {
        safeSend(ws, { type: 'fallback_http', reason });
      }
      callbacks?.onStatus?.(`fallback:${reason}`);
      finish('fallback', reason);
    };

    const finalizeDownload = () => {
      if (!metaReceived || !fileName || fileSize === 0) {
        fail('missing_metadata', true);
        return;
      }
      if (receivedBytes !== fileSize) {
        fail('size_mismatch', true);
        return;
      }
      const blob = new Blob(chunks, { type: 'application/octet-stream' });
      downloadBlob(blob, fileName);
      finish('success');
    };

    const handleBinary = async (data: ArrayBuffer | Blob) => {
      const buffer =
        data instanceof Blob ? await data.arrayBuffer() : (data as ArrayBuffer);
      chunks.push(buffer);
      receivedBytes += buffer.byteLength;
      if (fileSize > 0) {
        const percent = Math.round((receivedBytes / fileSize) * 100);
        callbacks?.onProgress?.(percent, receivedBytes, fileSize);
      }
    };

    const setupChannel = (channel: RTCDataChannel) => {
      dc = channel;
      dc.binaryType = 'arraybuffer';
      dc.onopen = () => {
        clearTimeout(timeout);
      };
      dc.onerror = () => fail('datachannel_error', true);
      dc.onmessage = event => {
        if (typeof event.data === 'string') {
          const message = parseSignalMessage(event.data);
          if (message?.type === 'p2p_done') {
            return;
          }
          try {
            const meta = JSON.parse(event.data) as {
              type: string;
              name?: string;
              size?: number;
            };
            if (meta.type === 'file_meta') {
              fileName = meta.name || fileName;
              fileSize = meta.size || fileSize;
              metaReceived = true;
              callbacks?.onMetadata?.(fileName, fileSize);
              if (endReceived) {
                finalizeDownload();
              }
            }
            if (meta.type === 'file_end') {
              if (!metaReceived) {
                endReceived = true;
                return;
              }
              finalizeDownload();
            }
          } catch {
            // ignore
          }
          return;
        }
        void handleBinary(event.data);
      };
    };

    ws.onopen = () => {
      safeSend(ws, { type: 'ready' });
    };

    ws.onmessage = async event => {
      const msg = parseSignalMessage(event.data);
      if (!msg) return;
      if (msg.type === 'offer') {
        if (!pc) {
          pc = setupPeerConnection(config, candidate => safeSend(ws, { type: 'ice', candidate }));
          pc.ondatachannel = event => setupChannel(event.channel);
          pc.onconnectionstatechange = () => {
            if (pc?.connectionState === 'failed') {
              fail('connection_failed', true);
            }
          };
        }
        await pc.setRemoteDescription(msg.sdp);
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        safeSend(ws, { type: 'answer', sdp: answer });
      } else if (msg.type === 'ice' && pc) {
        await pc.addIceCandidate(msg.candidate);
      } else if (msg.type === 'fallback_http') {
        fail('peer_fallback', false);
      }
    };

    ws.onclose = () => {
      if (!finished) {
        fail('signal_closed', false);
      }
    };
  });
};
