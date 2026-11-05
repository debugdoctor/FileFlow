<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { ArrowLeft, ArrowUpRight, Check, Copy, Files, Link2, Send } from 'lucide-vue-next';
import { copyText, mergeMessages, parseRoomCode, randomHex, textRequest, TextSessionError, type TextMessage, type TextSession, type TextSnapshot } from '@/utils/textSession';
import { splitMessageText } from '@/utils/messageText';

const session = ref<TextSession | null>(null);
const roomInput = ref(new URLSearchParams(location.search).get('room') ?? '');
const entryAction = new URLSearchParams(location.search).get('action');
const joinMode = entryAction === 'join' || new URLSearchParams(location.search).has('room');
const draft = ref('');
const messages = ref<TextMessage[]>([]);
const role = ref<TextSnapshot['role'] | null>(null);
const selfId = ref<number | null>(null);
const selfName = ref('');
const participantCount = ref(0);
const onlineCount = ref(0);
const maxParticipants = ref(32);
const isHost = computed(() => role.value === 'creator');
const busy = ref(false);
const sending = ref(false);
const error = ref('');
const notice = ref('');
const copied = ref('');
const connection = ref('');
const ended = ref(false);
const history = ref<HTMLElement>();
const draftBytes = computed(() => new TextEncoder().encode(draft.value).length);
const roomCode = computed(() => session.value?.roomId ?? '');
const shareLink = computed(() => session.value ? `${location.origin}/text?room=${session.value.roomId}` : '');
const shareDisplay = computed(() => shareLink.value.replace(/^https?:\/\//, ''));
const renderedMessages = computed(() => messages.value.map(message => {
  const parts = splitMessageText(message.text);
  const links = parts.filter(part => part.href);
  return { ...message, parts, link: links.length === 1 && parts.every(part => part.href || !part.text.trim()) ? links[0] : null };
}));
const canSend = computed(() => selfId.value !== null && draft.value.trim().length > 0 && draftBytes.value <= 65536 && !sending.value && !ended.value);
const status = computed(() => ended.value ? '会话已结束' : connection.value || `${participantCount.value} / ${maxParticipants.value} 人 · ${onlineCount.value} 人在线`);
let cursor = 0;
let pollTimer: ReturnType<typeof setTimeout> | undefined;
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
let copyTimer: ReturnType<typeof setTimeout> | undefined;
let controller: AbortController | undefined;
let pending: { id: string; text: string } | undefined;
let generation = 0;
let storageAvailable = true;

const storageKey = (id: string) => `fileflow-text:${id}`;
const createKey = 'fileflow-text:pending-create';
function remember(value: TextSession) {
  // Persist before a mutating request so retrying a lost response keeps the same participant.
  try { sessionStorage.setItem(storageKey(value.roomId), value.token); }
  catch { storageAvailable = false; }
}
function forget(id: string) {
  try { sessionStorage.removeItem(storageKey(id)); } catch { /* Storage may be disabled. */ }
}
function stop() {
  generation++;
  if (pollTimer) clearTimeout(pollTimer);
  controller?.abort();
}
function terminate(message: string) {
  stop();
  ended.value = true;
  error.value = message;
  if (session.value) forget(session.value.roomId);
}
async function showMessages(incoming: TextMessage[]) {
  const element = history.value;
  const nearBottom = !element || element.scrollHeight - element.scrollTop - element.clientHeight < 100;
  messages.value = mergeMessages(messages.value, incoming);
  if (nearBottom) { await nextTick(); history.value?.scrollTo({ top: history.value.scrollHeight }); }
}
async function poll(version: number) {
  const active = session.value;
  if (!active || ended.value || version !== generation) return;
  try {
    const snapshot = await textRequest<TextSnapshot>(`/${active.roomId}?since=${cursor}`, { token: active.token, signal: controller?.signal });
    if (version !== generation) return;
    role.value = snapshot.role;
    selfId.value = snapshot.self_id;
    selfName.value = snapshot.self_name;
    participantCount.value = snapshot.participant_count;
    onlineCount.value = snapshot.online_count;
    maxParticipants.value = snapshot.max_participants;
    connection.value = '';
    if (cursor > 0 && snapshot.oldest_seq > cursor + 1) flash('较早的消息已过期，已同步当前保留的记录');
    cursor = snapshot.latest_seq;
    await showMessages(snapshot.messages);
  } catch (cause) {
    if (version !== generation) return;
    if (cause instanceof TextSessionError && [401, 403, 404].includes(cause.status)) {
      terminate(cause.message); return;
    }
    connection.value = '连接中断，正在重试…';
  }
  if (version === generation) pollTimer = setTimeout(() => void poll(version), 1000);
}
function activate(value: TextSession) {
  stop();
  session.value = value;
  ended.value = false;
  error.value = '';
  connection.value = '正在连接…';
  messages.value = [];
  role.value = null;
  selfId.value = null;
  selfName.value = '';
  participantCount.value = 0;
  onlineCount.value = 0;
  cursor = 0;
  controller = new AbortController();
  window.history.replaceState(null, '', `/text?room=${value.roomId}`);
  if (!storageAvailable) flash('浏览器无法保存会话凭据，请保持此页面开启');
  void poll(generation);
}
async function openRoom(create: boolean) {
  if (busy.value) return;
  error.value = '';
  let id = create ? '' : parseRoomCode(roomInput.value);
  if (!create && !id) { error.value = '请输入 5 位房间码或完整分享链接'; return; }
  busy.value = true;
  let token: string | null = null;
  try { token = sessionStorage.getItem(create ? createKey : storageKey(id)); } catch { storageAvailable = false; }
  const credential = token && /^[a-f0-9]{64}$/.test(token) ? token : randomHex(32);
  try {
    if (create) {
      // The server recovers the same room for this token if the response is lost.
      try { sessionStorage.setItem(createKey, credential); } catch { storageAvailable = false; }
      const result = await textRequest<{ room_id: string }>('', { body: { token: credential } });
      id = result.room_id;
    } else {
      remember({ roomId: id, token: credential });
      await textRequest(`/${id}/join`, { body: { token: credential } });
    }
    const value = { roomId: id, token: credential };
    remember(value);
    if (create) { try { sessionStorage.removeItem(createKey); } catch { /* The active room is already remembered. */ } }
    activate(value);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : '无法连接会话，请重试';
    if (id && cause instanceof TextSessionError && [400, 403, 404, 409].includes(cause.status)) forget(id);
  } finally { busy.value = false; }
}
async function send() {
  if (!canSend.value || !session.value) return;
  const active = session.value;
  if (!pending || pending.text !== draft.value) pending = { id: randomHex(16), text: draft.value };
  const payload = pending;
  sending.value = true;
  error.value = '';
  try {
    const message = await textRequest<TextMessage>(`/${active.roomId}/messages`, { token: active.token, body: payload });
    await showMessages([message]);
    if (draft.value === payload.text) draft.value = '';
    pending = undefined;
  } catch (cause) {
    if (cause instanceof TextSessionError && [401, 403, 404].includes(cause.status)) terminate(cause.message);
    else error.value = '发送未确认，文本已保留；点击发送可重试';
  } finally { sending.value = false; }
}
function shortcut(event: KeyboardEvent) {
  if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && !event.isComposing) {
    event.preventDefault(); void send();
  }
}
function flash(message: string) {
  notice.value = message;
  if (noticeTimer) clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => { notice.value = ''; }, 4000);
}
async function copy(value: string, key = 'message') {
  try {
    await copyText(value);
    copied.value = key;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied.value = ''; }, 2500);
    flash('已复制');
  }
  catch (cause) { error.value = cause instanceof Error ? cause.message : '复制失败，请手动复制'; }
}
async function closeRoom() {
  if (!session.value || busy.value || sending.value || (!ended.value && selfId.value === null)) return;
  if (!ended.value && !window.confirm(isHost.value ? '结束后所有成员将无法继续聊天，服务器上的文本记录会被清除。确定结束房间？' : '退出后会腾出一个名额，其他成员仍可继续聊天。确定退出房间？')) return;
  busy.value = true;
  try {
    if (!ended.value) await textRequest(`/${session.value.roomId}/${isHost.value ? 'close' : 'leave'}`, { token: session.value.token, body: {} });
    forget(session.value.roomId);
    stop();
    session.value = null;
    ended.value = false;
    messages.value = [];
    draft.value = '';
    pending = undefined;
    error.value = '';
    roomInput.value = '';
    role.value = null;
    selfId.value = null;
    selfName.value = '';
    participantCount.value = 0;
    onlineCount.value = 0;
    window.history.replaceState(null, '', '/text');
  } catch (cause) {
    if (cause instanceof TextSessionError && cause.status === 404) terminate(cause.message);
    else error.value = isHost.value ? '未能结束房间，请重试' : '未能退出房间，请重试';
  } finally { busy.value = false; }
}
onMounted(() => {
  if (entryAction === 'create') { void openRoom(true); return; }
  // Opening an invitation does not consume a place until the user chooses to join.
  const id = parseRoomCode(roomInput.value);
  if (!id) return;
  try {
    const token = sessionStorage.getItem(storageKey(id));
    if (token && /^[a-f0-9]{64}$/.test(token)) activate({ roomId: id, token });
  } catch { storageAvailable = false; }
});
onBeforeUnmount(() => { stop(); if (noticeTimer) clearTimeout(noticeTimer); if (copyTimer) clearTimeout(copyTimer); });
</script>

<template>
  <div class="page">
    <header><a href="/" class="logo"><span class="logo-mark"><Files :size="23" /></span>FileFlow</a><a href="/" class="back"><ArrowLeft :size="16" /> 返回首页</a></header>
    <main>
      <section v-if="!session" class="panel lobby" aria-label="创建或加入文本会话">
        <template v-if="!joinMode"><h2>开启一个聊天房间</h2><p>分享链接或房间码，最多 32 人一起聊，每个人都有随机匿名昵称。</p><button class="primary create" :disabled="busy" @click="openRoom(true)">{{ busy ? '正在创建…' : '创建聊天' }}</button><div class="divider">或加入已有聊天</div></template>
        <template v-else><h2>加入聊天</h2><p>粘贴分享链接或输入房间码，加入后自动分配匿名昵称。</p></template>
        <form @submit.prevent="openRoom(false)"><label for="room-code">房间码或分享链接</label><div class="join"><input id="room-code" v-model="roomInput" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="输入 5 位房间码，如 a3b7k" :disabled="busy" required /><button :disabled="busy || !roomInput.trim()">{{ busy ? '正在加入…' : '加入聊天' }}</button></div></form>
      </section>
      <section v-else class="panel session" aria-label="多人匿名聊天">
        <div class="session-bar"><div class="room-identity"><span class="small-label">房间码</span><button class="room-code" :disabled="ended" :aria-label="copied === 'code' ? '房间码已复制' : '复制房间码'" title="点击复制房间码" @click="copy(roomCode, 'code')"><strong>{{ roomCode }}</strong><Check v-if="copied === 'code'" :size="16" /><Copy v-else :size="15" /></button></div><div class="room-presence"><span class="status" :class="{ online: !ended && !connection }" role="status"><i aria-hidden="true" />{{ status }}</span><span v-if="selfName" class="self-name">你是 <b>{{ selfName }}</b>{{ isHost ? ' · 房主' : '' }}</span></div></div>
        <div class="share"><div class="invite"><span class="invite-icon"><Link2 :size="19" /></span><div class="invite-info"><span class="invite-label">邀请朋友加入</span><span class="invite-address" :title="shareLink">{{ shareDisplay }}</span></div><button class="copy-invite" :class="{ copied: copied === 'invite' }" :disabled="ended" @click="copy(shareLink, 'invite')"><Check v-if="copied === 'invite'" :size="15" /><Copy v-else :size="15" />{{ copied === 'invite' ? '已复制' : '复制链接' }}</button></div><p class="share-hint">分享链接或 5 位房间码，最多 {{ maxParticipants }} 人一起聊</p></div>
        <div ref="history" class="messages" role="log" aria-label="文本记录" aria-live="polite" aria-relevant="additions">
          <p v-if="!messages.length" class="empty">{{ participantCount > 1 ? '大家已加入，发送第一条文本吧。' : '邀请朋友加入，也可以先发送文本。' }}</p>
          <article v-for="message in renderedMessages" :key="message.seq" class="message" :class="{ mine: message.author === selfId, 'has-link-card': message.link }">
            <div class="message-meta"><span>{{ message.author_name }}{{ message.author === selfId ? '（我）' : '' }} · {{ new Date(message.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) }}</span><button class="copy-message" aria-label="复制这条文本" @click="copy(message.text, `message-${message.seq}`)"><Check v-if="copied === `message-${message.seq}`" :size="14" /><Copy v-else :size="14" />{{ copied === `message-${message.seq}` ? '已复制' : '复制' }}</button></div>
            <a v-if="message.link" class="link-card" :href="message.link.href" target="_blank" rel="noopener noreferrer" :title="message.link.text"><span class="link-icon"><Link2 :size="20" /></span><span class="link-details"><span class="link-domain">{{ message.link.host }}</span><span class="link-address">{{ message.link.text }}</span><span class="link-action">打开链接 <ArrowUpRight :size="12" /></span></span><ArrowUpRight class="link-arrow" :size="17" /></a>
            <div v-else class="message-body"><template v-for="(part, index) in message.parts" :key="index"><a v-if="part.href" class="inline-link" :href="part.href" target="_blank" rel="noopener noreferrer" :title="part.text">{{ part.text }}</a><template v-else>{{ part.text }}</template></template></div>
          </article>
        </div>
        <form class="composer" @submit.prevent="send"><label for="draft">发送文本</label><textarea id="draft" v-model="draft" rows="4" placeholder="粘贴链接、代码或任意文字…" :disabled="ended" @keydown="shortcut" /><div class="composer-actions"><span :class="{ over: draftBytes > 65536 }">{{ (draftBytes / 1024).toFixed(1) }} / 64 KiB · Ctrl / ⌘ + Enter 发送</span><button class="primary" :disabled="!canSend" type="submit"><Send :size="16" />{{ sending ? '发送中…' : '发送' }}</button></div></form>
        <div class="session-footer"><span>仅保留最近 100 条 / 256 KiB 文本</span><button :disabled="busy || sending || (!ended && selfId === null)" @click="closeRoom">{{ ended ? '返回房间入口' : isHost ? '结束房间' : '退出房间' }}</button></div>
      </section>
      <p v-if="error" class="error" role="alert">{{ error }}</p><p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <p class="hint">文本经服务器临时中转；全员离线 1 小时后过期，房主结束房间或服务重启会清空。关闭标签页仍占名额，请点击「退出房间」释放。</p>
    </main>
  </div>
</template>

<style scoped>
:global(body) { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #f8faff; color: #23324d; }
* { box-sizing: border-box; }
.page { min-height: 100vh; padding: 0 28px 28px; background: radial-gradient(ellipse at 85% 15%, #eeecff99, transparent 45%), #f8faff; }
header { max-width: 1040px; height: 92px; margin: auto; display: flex; align-items: center; justify-content: space-between; gap: 16px; }
a { color: inherit; text-decoration: none; }
.logo { display: flex; align-items: center; gap: 11px; font-size: 23px; font-weight: 750; letter-spacing: -.8px; }
.logo-mark { display: grid; place-items: center; width: 39px; height: 39px; border-radius: 12px; background: linear-gradient(145deg, #497ffa, #7866e9); color: white; }
.back { display: flex; align-items: center; gap: 7px; color: #71809b; font-size: 13px; }
main { max-width: 800px; margin: 20px auto 0; }
.lobby p { color: #7c89a0; font-size: 14px; line-height: 1.8; }
.panel { background: #fff; border: 1px solid #e1e8f6; border-radius: 18px; box-shadow: 0 12px 35px #526fa208; }
.lobby { max-width: 570px; margin: auto; padding: 32px; }
h2 { font-size: 20px; margin: 0; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; border: 1px solid #dce4f2; background: #f8faff; color: #526da1; border-radius: 9px; padding: 10px 15px; font: inherit; font-size: 13px; cursor: pointer; white-space: nowrap; }
button:hover { background: #eff4ff; }
button:disabled { opacity: .5; cursor: default; }
.primary { color: white; background: #537fe7; border-color: #537fe7; }
.primary:hover { background: #466fd1; }
.create { width: 100%; margin-top: 12px; padding: 13px; }
.divider { text-align: center; margin: 27px 0; color: #9aa5b8; font-size: 12px; }
label { display: block; margin-bottom: 10px; font-size: 13px; color: #62728f; }
.join { display: flex; gap: 10px; }
input, textarea { min-width: 0; border: 1px solid #dfe6f3; border-radius: 9px; padding: 11px 12px; color: #23324d; background: #fff; font: inherit; font-size: 14px; }
input { flex: 1; width: 100%; }
textarea { display: block; width: 100%; resize: vertical; min-height: 100px; line-height: 1.7; }
button:focus-visible, input:focus-visible, textarea:focus-visible, a:focus-visible { outline: 3px solid #a7bfff; outline-offset: 3px; }
.session { overflow: hidden; }
.session-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 22px 24px 16px; flex-wrap: wrap; }
.room-identity { display: flex; align-items: center; gap: 12px; }
.small-label { color: #7b89a0; font-size: 12px; }
.room-code { padding: 6px 9px 6px 12px; background: #f1f5ff; border-color: #e5ecfc; gap: 15px; color: #5972ab; }
strong { color: #354e7c; font-size: 23px; letter-spacing: 4px; font-family: ui-monospace, monospace; }
.status { display: inline-flex; align-items: center; gap: 7px; color: #8b94a7; font-size: 12px; }
.status i { width: 6px; height: 6px; background: currentColor; border-radius: 50%; flex-shrink: 0; }
.room-presence { display: flex; flex-direction: column; gap: 6px; }
.self-name { color: #8b97ac; font-size: 11px; overflow-wrap: anywhere; }
.self-name b { color: #64799d; font-weight: 500; }
.online { color: #389a78; }
.share { padding: 0 24px 18px; }
.invite { display: flex; align-items: center; gap: 12px; min-width: 0; padding: 12px; border: 1px solid #e7ecf6; border-radius: 12px; background: #fbfcff; }
.invite-icon { display: grid; place-items: center; flex: 0 0 36px; height: 36px; color: #7185b6; background: #edf2ff; border-radius: 10px; }
.invite-info { display: flex; flex: 1; flex-direction: column; min-width: 0; gap: 4px; }
.invite-label { color: #4c5f80; font-size: 12px; font-weight: 600; }
.invite-address { color: #8592a9; font-size: 12px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.copy-invite { flex-shrink: 0; background: white; padding: 9px 12px; }
.copy-invite.copied { color: #389a78; border-color: #cce8dc; }
.share-hint { margin: 9px 2px 0; font-size: 11px; line-height: 1.7; color: #8d9ab0; }
.messages { height: min(42vh, 420px); min-height: 220px; overflow: auto; padding: 20px 24px; background: #f8faff; border-block: 1px solid #edf1f9; }
.empty { color: #97a1b5; text-align: center; font-size: 13px; padding: 50px 0; line-height: 1.8; }
.message { max-width: 88%; width: fit-content; min-width: 180px; margin: 0 auto 16px 0; padding: 12px 15px; border: 1px solid #e4eaf5; border-radius: 12px; background: white; }
.message.mine { margin-left: auto; margin-right: 0; background: #edf3ff; border-color: #dce7ff; }
.message-meta { display: flex; align-items: center; justify-content: space-between; gap: 18px; color: #8693aa; font-size: 11px; margin-bottom: 8px; }
.message-meta > span { min-width: 0; overflow-wrap: anywhere; }
.copy-message { padding: 2px; border: 0; background: transparent; font-size: 11px; }
.message-body { white-space: pre-wrap; overflow-wrap: anywhere; font-family: inherit; font-size: 14px; line-height: 1.7; margin: 0; }
.inline-link { color: #446dca; text-decoration: underline; text-decoration-color: #a9bde9; text-underline-offset: 3px; }
.inline-link:hover { color: #2b50a6; text-decoration-color: currentColor; }
.message.has-link-card { width: 360px; }
.link-card { display: flex; align-items: flex-start; gap: 12px; padding: 13px; border: 1px solid #e1e8f5; border-radius: 10px; background: #ffffffb3; transition: border-color .15s, background .15s; }
.link-card:hover { border-color: #afc3ec; background: #fff; }
.link-icon { display: grid; place-items: center; flex: 0 0 34px; height: 34px; border-radius: 9px; color: #647fbc; background: #edf2fc; }
.link-details { flex: 1; min-width: 0; }
.link-domain { display: block; color: #344f81; font-size: 14px; font-weight: 650; line-height: 1.5; overflow-wrap: anywhere; }
.link-address { display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; color: #8492aa; font-size: 12px; line-height: 1.6; margin-top: 4px; }
.link-action { display: flex; align-items: center; gap: 3px; color: #6381bd; font-size: 11px; margin-top: 9px; }
.link-arrow { flex-shrink: 0; color: #8a9ec4; margin-top: 2px; }
.composer { padding: 20px 24px 14px; }
.composer-actions { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-top: 10px; }
.composer-actions span { color: #8b97ac; font-size: 11px; }
.composer-actions span.over, .error { color: #c45260; }
.session-footer { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 0 24px 18px; color: #9aa5b7; font-size: 11px; }
.session-footer button { padding: 5px 0; border: 0; background: none; color: #a17883; font-size: 12px; }
.hint { text-align: center; color: #95a0b4; font-size: 11px; margin: 20px 0 0; line-height: 1.8; }
.error, .notice { text-align: center; font-size: 13px; line-height: 1.7; margin: 16px 0; overflow-wrap: anywhere; }
.notice { color: #3a9679; }
@media (max-width: 600px) { .page { padding: 0 14px 22px; } header { height: 78px; } main { margin-top: 8px; } .lobby { padding: 24px 20px; } .session-bar { padding: 18px 16px 14px; gap: 10px; } .room-identity { gap: 8px; } .room-code { gap: 10px; } strong { font-size: 21px; letter-spacing: 3px; } .status { font-size: 11px; } .share { padding: 0 16px 16px; } .invite { gap: 9px; padding: 10px; } .invite-icon { display: none; } .copy-invite { padding: 9px 10px; font-size: 12px; } .messages { padding: 16px; } .composer { padding: 16px; } .session-footer { padding: 0 16px 16px; } .message { max-width: 95%; } .link-card { gap: 9px; padding: 10px; } .link-arrow { display: none; } .composer-actions span { max-width: 160px; line-height: 1.6; } }
</style>
