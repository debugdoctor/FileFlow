<script setup lang="ts">
import { ArrowUpRight, Upload, Download, Files, MessagesSquare, Plus, LogIn } from 'lucide-vue-next';
</script>

<template>
  <div class="home">
    <header><a href="/" class="logo" aria-label="FileFlow 首页"><span class="logo-mark"><Files :size="23" :stroke-width="1.7" /></span>FileFlow</a><span class="caption">连接设备，分享文件与文字</span></header>
    <main>
      <div class="headline"><h1>连接彼此，<span>分享更轻松。</span></h1><p>创建一个房间，或加入对方的房间。传文件，也聊几句。</p></div>
      <div class="entries">
        <section class="entry create" aria-labelledby="create-title">
          <span class="entry-icon"><Plus :size="29" :stroke-width="1.6" /></span><h2 id="create-title">创建房间</h2><p>从这里发起，邀请另一台设备连接</p>
          <div class="options"><a href="/upload" class="option"><Upload :size="20" :stroke-width="1.7" /><span><strong>发送文件</strong><small>选择文件，生成分享链接</small></span><ArrowUpRight :size="17" /></a><a href="/text?action=create" class="option"><MessagesSquare :size="20" :stroke-width="1.7" /><span><strong>创建聊天</strong><small>开启双向文字聊天，分享房间码</small></span><ArrowUpRight :size="17" /></a></div>
        </section>
        <section class="entry join" aria-labelledby="join-title">
          <span class="entry-icon"><LogIn :size="29" :stroke-width="1.6" /></span><h2 id="join-title">加入房间</h2><p>已有链接或房间码？在这里连接对方</p>
          <div class="options"><a href="/download" class="option"><Download :size="20" :stroke-width="1.7" /><span><strong>接收文件</strong><small>输入接收码，下载对方的文件</small></span><ArrowUpRight :size="17" /></a><a href="/text?action=join" class="option"><MessagesSquare :size="20" :stroke-width="1.7" /><span><strong>加入聊天</strong><small>输入房间码，即可互传文字</small></span><ArrowUpRight :size="17" /></a></div>
        </section>
      </div>
    </main>
    <footer>文件传输时请保持双方页面开启 · 聊天房间内可双向发送文字</footer>
  </div>
</template>

<style scoped>
:global(body) { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #f8faff; color: #23324d; }
* { box-sizing: border-box; }
.home { min-height: 100vh; display: flex; flex-direction: column; padding: 0 32px; background: radial-gradient(ellipse at 85% 15%, #eeecff99, transparent 45%), radial-gradient(ellipse at 15% 65%, #eaf2ff99, transparent 45%), #f8faff; }
header { width: 100%; max-width: 1040px; margin: 0 auto; height: 92px; display: flex; align-items: center; justify-content: space-between; gap: 20px; }
a { color: inherit; text-decoration: none; }
a:focus-visible { outline: 3px solid #8cafff; outline-offset: 6px; }
.logo { display: flex; align-items: center; gap: 11px; font-size: 23px; font-weight: 750; letter-spacing: -.8px; }
.logo-mark { display: grid; place-items: center; width: 39px; height: 39px; border-radius: 12px; background: linear-gradient(145deg, #497ffa, #7866e9); color: white; box-shadow: 0 5px 14px #5378ea24; }
.caption { color: #8994a9; font-size: 13px; }
main { flex: 1; display: flex; flex-direction: column; justify-content: center; width: 100%; max-width: 800px; margin: 0 auto; padding: 54px 0 76px; }
.headline { text-align: center; margin-bottom: 42px; }
h1 { margin: 0 0 17px; font-size: clamp(32px, 4.5vw, 46px); line-height: 1.4; letter-spacing: -1.5px; font-weight: 700; }
h1 span { background: linear-gradient(100deg, #377ff6, #8669df); background-clip: text; -webkit-text-fill-color: transparent; }
.headline p { color: #8893a8; font-size: 14px; line-height: 1.8; margin: 0; }
.entries { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }
.entry { padding: 32px; border: 1px solid #e1e8f6; background: #ffffffdd; border-radius: 18px; transition: border-color .2s, box-shadow .2s, transform .2s; }
.entry-icon { display: grid; place-items: center; width: 57px; height: 57px; border-radius: 15px; color: #5487ef; background: #edf3ff; }
.join .entry-icon { color: #9576d6; background: #f1ecfb; }
h2 { margin: 23px 0 10px; font-size: 21px; font-weight: 650; }
.entry p { color: #8893a8; font-size: 12px; line-height: 1.8; margin: 0; }
.options { display: grid; gap: 10px; margin-top: 26px; padding-top: 22px; border-top: 1px solid #edf1f9; }
.option { display: flex; align-items: center; gap: 13px; padding: 15px 13px; border: 1px solid #e8eef9; border-radius: 11px; color: #5d83cb; background: #f9fbff; transition: background .2s, border-color .2s; }
.option:hover { background: #eff4ff; border-color: #bfd0f4; }
.option span { flex: 1; }
.option strong { display: block; color: #344765; font-size: 14px; font-weight: 600; }
.option small { display: block; color: #929db0; font-size: 11px; line-height: 1.6; margin-top: 5px; }
.join .option { color: #9478c8; }
.join .option:hover { background: #f6f2fc; border-color: #d3c5ee; }
footer { text-align: center; padding: 0 0 28px; font-size: 11px; color: #a0a9ba; }
@media (max-width: 600px) { .home { padding: 0 22px; } header { height: 80px; } .caption { display: none; } main { padding: 40px 0; } .headline { margin-bottom: 28px; } h1 { font-size: 31px; letter-spacing: -1px; } .headline p { font-size: 12px; } .entries { grid-template-columns: 1fr; gap: 16px; } .entry { padding: 25px; } h2 { margin-top: 18px; } .options { margin-top: 22px; padding-top: 16px; } }
@media (prefers-reduced-motion: reduce) { .entry, .option { transition: none; } }
</style>
