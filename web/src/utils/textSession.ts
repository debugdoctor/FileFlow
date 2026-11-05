import { fetchWithTimeout } from './http';

export interface TextSession { roomId: string; token: string }
export interface TextMessage {
  seq: number;
  id: string;
  author: number;
  author_name: string;
  text: string;
  created_at: number;
}
export interface TextSnapshot {
  role: 'creator' | 'guest';
  self_id: number;
  self_name: string;
  participant_count: number;
  online_count: number;
  max_participants: number;
  messages: TextMessage[];
  latest_seq: number;
  oldest_seq: number;
}

export class TextSessionError extends Error {
  constructor(message: string, public status: number) { super(message); }
}

export const randomHex = (bytes: number) => Array.from(crypto.getRandomValues(new Uint8Array(bytes)), byte => byte.toString(16).padStart(2, '0')).join('');

export function parseRoomCode(input: string): string {
  let code = input.trim();
  if (/^https?:\/\//i.test(code)) {
    try { code = new URL(code).searchParams.get('room') ?? ''; }
    catch { return ''; }
  }
  code = code.toLowerCase();
  return /^[a-z0-9]{5}$/.test(code) ? code : '';
}

export async function textRequest<T>(path: string, options: { token?: string; body?: unknown; signal?: AbortSignal } = {}): Promise<T> {
  const headers: Record<string, string> = {};
  if (options.token) headers.Authorization = `Bearer ${options.token}`;
  if (options.body !== undefined) headers['Content-Type'] = 'application/json';
  return fetchWithTimeout(`/api/fileflow/text/rooms${path}`, {
    method: options.body === undefined ? 'GET' : 'POST', headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body), signal: options.signal,
    cache: 'no-store',
  }, 8000, async response => {
    const result = await response.json() as { success?: boolean; message?: string; data: T };
    if (!response.ok || !result.success) throw new TextSessionError(result.message || '请求失败，请重试', response.status);
    return result.data;
  });
}

// Sending a message never advances the receive cursor: an earlier peer message may still be in flight.
export function mergeMessages(current: TextMessage[], incoming: TextMessage[]): TextMessage[] {
  const messages = new Map(current.map(message => [message.seq, message]));
  for (const message of incoming) messages.set(message.seq, message);
  const sorted = [...messages.values()].sort((a, b) => a.seq - b.seq);
  let bytes = 0;
  const encoder = new TextEncoder();
  const retained: TextMessage[] = [];
  for (const message of sorted.reverse()) {
    bytes += encoder.encode(message.text).length;
    if (retained.length >= 100 || bytes > 256 * 1024) break;
    retained.push(message);
  }
  return retained.reverse();
}

export async function copyText(text: string): Promise<void> {
  try {
    if (navigator.clipboard?.writeText) { await navigator.clipboard.writeText(text); return; }
  } catch { /* HTTP deployments and denied permissions can still use a user-initiated copy. */ }
  const previous = document.activeElement;
  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.style.cssText = 'position:fixed;opacity:0;left:0;top:0';
  document.body.appendChild(textarea);
  textarea.select();
  try {
    if (!document.execCommand('copy')) throw new Error('复制失败，请手动选择文本复制');
  } finally {
    textarea.remove();
    if (previous instanceof HTMLElement) previous.focus();
  }
}
