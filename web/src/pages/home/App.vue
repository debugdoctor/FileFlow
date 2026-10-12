<script setup lang="ts">
import { ArrowUpRight, Upload, Download, Files } from 'lucide-vue-next';
</script>

<template>
  <div class="home">
    <header><a href="/" class="logo" aria-label="FileFlow 首页"><span class="logo-mark"><Files :size="23" :stroke-width="1.7" /></span>FileFlow</a><span class="caption">轻量直连，快速传文件</span></header>
    <main>
      <div class="headline"><h1>分享文件，<span>轻松一点。</span></h1><p>选好文件，发出链接。让文件顺畅抵达另一端。</p></div>
      <div class="entries">
        <a href="/upload" class="entry upload"><span class="entry-icon"><Upload :size="29" :stroke-width="1.6" /></span><h2>我要上传</h2><p>生成下载链接，分享给接收方</p><span class="entry-action">进入上传 <ArrowUpRight :size="18" /></span></a>
        <a href="/download" class="entry download"><span class="entry-icon"><Download :size="29" :stroke-width="1.6" /></span><h2>我要下载</h2><p>打开分享链接即可下载，也可输入接收码</p><span class="entry-action">进入下载 <ArrowUpRight :size="18" /></span></a>
      </div>
    </main>
    <footer>传输过程中，请保持发送页面开启</footer>
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
.entry:hover { border-color: #bccff5; box-shadow: 0 12px 35px #526fa20b; transform: translateY(-3px); }
.entry-icon { display: grid; place-items: center; width: 57px; height: 57px; border-radius: 15px; color: #5487ef; background: #edf3ff; }
.download .entry-icon { color: #9576d6; background: #f1ecfb; }
.download:hover { border-color: #d3c5ee; }
h2 { margin: 23px 0 10px; font-size: 21px; font-weight: 650; }
.entry p { color: #8893a8; font-size: 12px; line-height: 1.8; margin: 0; }
.entry-action { display: flex; align-items: center; justify-content: space-between; margin-top: 29px; padding-top: 20px; border-top: 1px solid #edf1f9; color: #5d83cb; font-size: 13px; font-weight: 600; }
.download .entry-action { color: #9478c8; }
footer { text-align: center; padding: 0 0 28px; font-size: 11px; color: #a0a9ba; }
@media (max-width: 600px) { .home { padding: 0 22px; } header { height: 80px; } .caption { display: none; } main { padding: 40px 0; } .headline { margin-bottom: 28px; } h1 { font-size: 31px; letter-spacing: -1px; } .headline p { font-size: 12px; } .entries { grid-template-columns: 1fr; gap: 16px; } .entry { padding: 25px; } h2 { margin-top: 18px; } .entry-action { margin-top: 22px; padding-top: 16px; } }
@media (prefers-reduced-motion: reduce) { .entry { transition: none; } }
</style>
