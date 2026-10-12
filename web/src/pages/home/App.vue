<script setup lang="ts">
import { ArrowUpRight, ArrowRight, Upload, Download, Files, Link2, Monitor, Smartphone, Check, FolderOpen } from 'lucide-vue-next';
</script>

<template>
  <div class="home">
    <header class="navigation">
      <a href="/" class="logo" aria-label="FileFlow 首页"><span class="logo-mark"><Files :size="23" :stroke-width="1.7" /></span>FileFlow</a>
      <span class="nav-caption">文件，让分享更简单</span>
      <a href="/download" class="nav-link">进入下载 <ArrowUpRight :size="16" /></a>
    </header>

    <main>
      <section class="hero" aria-labelledby="hero-title">
        <div class="hero-copy">
          <div class="eyebrow"><span></span> 轻量直连，快速传文件</div>
          <h1 id="hero-title">分享文件，<br /><span class="gradient-text">轻松一点。</span></h1>
          <p class="hero-description">选好文件，发出链接。<br />让文件顺畅抵达另一端。</p>
          <div class="hero-actions">
            <a href="/upload" class="primary-action">进入上传 <ArrowUpRight :size="19" /></a>
            <a href="#how-it-works" class="text-action">如何使用 <ArrowRight :size="16" /></a>
          </div>
          <div class="hero-notes"><span><Check :size="14" /> 无需注册</span><span><Check :size="14" /> 支持文件夹</span><span><Check :size="14" /> 跨设备传输</span></div>
        </div>

        <div class="transfer-visual" aria-hidden="true">
          <div class="orbit orbit-one"></div><div class="orbit orbit-two"></div>
          <div class="device sender"><div class="device-icon"><Monitor :size="24" :stroke-width="1.5" /></div><span>你的设备</span><i class="online-dot"></i></div>
          <div class="connection connection-left"></div><div class="connection connection-right"></div>
          <div class="file-preview">
            <div class="preview-top"><span class="preview-label">准备好分享</span><span class="preview-dots">•••</span></div>
            <div class="folder-icon"><FolderOpen :size="42" :stroke-width="1.3" /></div>
            <strong>生成下载链接</strong>
            <span class="preview-description">照片 · 文档 · 文件夹</span>
            <div class="preview-link"><Link2 :size="15" /><span>fileflow / share</span><span class="link-check"><Check :size="12" /></span></div>
          </div>
          <div class="device receiver"><div class="device-icon"><Smartphone :size="24" :stroke-width="1.5" /></div><span>另一端</span><i class="online-dot"></i></div>
          <div class="floating-label"><Link2 :size="14" /> 分享给接收方</div>
        </div>
      </section>

      <section class="entry-section" aria-labelledby="entry-title">
        <div class="section-caption"><h2 id="entry-title">上传与下载</h2></div>
        <div class="entry-grid">
          <a href="/upload" class="entry-card upload-entry">
            <span class="entry-icon"><Upload :size="27" :stroke-width="1.6" /></span>
            <div class="entry-copy"><h3>我要上传</h3><p>生成下载链接，分享给接收方</p><span class="entry-cta">进入上传 <ArrowRight :size="16" /></span></div>
            <ArrowUpRight :size="21" class="entry-arrow" />
          </a>
          <a href="/download" class="entry-card download-entry">
            <span class="entry-icon"><Download :size="27" :stroke-width="1.6" /></span>
            <div class="entry-copy"><h3>我要下载</h3><p>打开分享链接即可下载，也可输入接收码</p><span class="entry-cta">进入下载 <ArrowRight :size="16" /></span></div>
            <ArrowUpRight :size="21" class="entry-arrow" />
          </a>
        </div>
      </section>

      <section id="how-it-works" class="steps" aria-label="使用步骤">
        <div class="step"><span class="step-number">01</span><div><h3>选择文件</h3><p>单个文件、多个文件或整个文件夹</p></div></div>
        <ArrowRight :size="18" class="step-arrow" />
        <div class="step"><span class="step-number">02</span><div><h3>分享链接</h3><p>复制下载链接发给接收方</p></div></div>
        <ArrowRight :size="18" class="step-arrow" />
        <div class="step"><span class="step-number">03</span><div><h3>开始接收</h3><p>对方打开链接，点击开始下载</p></div></div>
      </section>
    </main>
    <footer><span>FileFlow <span class="footer-divider">/</span> 轻量直连，快速传文件</span><span>传输过程中，请保持发送页面开启</span></footer>
  </div>
</template>

<style scoped>
:global(body) { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; color: #23324d; background: #f7f9fe; }
* { box-sizing: border-box; }
.home { min-height: 100vh; padding: 0 40px; background: radial-gradient(ellipse at 83% 18%, #e9eaffaa, transparent 43%), radial-gradient(ellipse at 12% 43%, #eaf3ff99, transparent 47%), #f8faff; }
a { color: inherit; text-decoration: none; }
a:focus-visible { outline: 3px solid #8cafff; outline-offset: 6px; }
.navigation, main, footer { width: 100%; max-width: 1120px; margin: 0 auto; }
.navigation { height: 94px; display: flex; align-items: center; justify-content: space-between; gap: 20px; border-bottom: 1px solid #e4eaf5; }
.logo { display: flex; align-items: center; gap: 11px; font-size: 23px; font-weight: 750; letter-spacing: -.8px; }
.logo-mark { display: grid; place-items: center; width: 39px; height: 39px; border-radius: 12px; background: linear-gradient(145deg, #497ffa, #7866e9); color: white; box-shadow: 0 5px 14px #5378ea24; }
.nav-caption { color: #8791a6; font-size: 13px; letter-spacing: 1px; }
.nav-link { display: flex; align-items: center; gap: 8px; font-size: 14px; color: #506383; }
.hero { display: grid; grid-template-columns: 1fr 1fr; align-items: center; gap: 30px; padding: 68px 0 60px; }
.eyebrow { display: flex; align-items: center; gap: 9px; color: #5979bb; font-size: 12px; font-weight: 600; letter-spacing: 2px; }
.eyebrow span { width: 7px; height: 7px; background: #678cf3; border-radius: 50%; box-shadow: 0 0 0 4px #678cf312; }
h1 { margin: 23px 0 20px; font-size: clamp(42px, 4.7vw, 60px); line-height: 1.28; letter-spacing: -2px; font-weight: 750; color: #202e49; }
.gradient-text { background: linear-gradient(100deg, #377ff6, #8669df); background-clip: text; -webkit-text-fill-color: transparent; }
.hero-description { color: #7c879d; font-size: 16px; line-height: 1.9; margin: 0; }
.hero-actions { display: flex; align-items: center; gap: 28px; margin-top: 28px; }
.primary-action { display: inline-flex; align-items: center; gap: 27px; padding: 15px 23px; color: white; font-size: 14px; font-weight: 600; background: #457ef4; border-radius: 11px; box-shadow: 0 7px 20px #457ef426; transition: background .2s, transform .2s; }
.primary-action:hover { color: white; background: #326ce4; transform: translateY(-2px); }
.text-action { display: flex; align-items: center; gap: 9px; color: #74819c; font-size: 13px; }
.hero-notes { display: flex; flex-wrap: wrap; gap: 21px; margin-top: 25px; font-size: 12px; color: #8b95a7; }
.hero-notes span { display: flex; align-items: center; gap: 5px; }
.hero-notes svg { color: #789ae8; }
.transfer-visual { position: relative; height: 350px; display: grid; place-items: center; }
@media (max-width: 760px) { .transfer-visual { overflow: clip; } }
.orbit { position: absolute; border: 1px solid #dbe3fa; border-radius: 50%; transform: rotate(-25deg); }
.orbit-one { width: 350px; height: 290px; }
.orbit-two { width: 450px; height: 350px; border-style: dashed; border-color: #dee5f6; }
.file-preview { position: relative; z-index: 1; width: 248px; padding: 20px; display: flex; align-items: center; flex-direction: column; background: #ffffffef; border: 1px solid white; border-radius: 20px; box-shadow: 0 22px 60px #4e6aba15, 0 3px 10px #4e6aba08; transform: rotate(-5deg); }
.preview-top { display: flex; justify-content: space-between; width: 100%; align-items: center; }
.preview-label { color: #8594af; font-size: 10px; }
.preview-dots { color: #bec9df; letter-spacing: 2px; }
.folder-icon { display: grid; place-items: center; color: #5a87ef; background: linear-gradient(145deg, #edf3ff, #f2efff); border: 1px solid #e5eafa; border-radius: 18px; width: 80px; height: 80px; margin: 20px 0; }
.file-preview strong { font-size: 16px; font-weight: 600; }
.preview-description { color: #9aa4b9; font-size: 11px; margin-top: 8px; }
.preview-link { display: flex; align-items: center; gap: 8px; color: #7c93c0; padding: 11px; border: 1px solid #e6edfb; background: #f6f9ff; border-radius: 8px; margin-top: 22px; width: 100%; font-size: 11px; }
.link-check { display: grid; place-items: center; margin-left: auto; color: #6087dc; background: #e6edfc; width: 19px; height: 19px; border-radius: 50%; }
.device { z-index: 2; position: absolute; display: flex; align-items: center; gap: 10px; background: white; border: 1px solid #e9edfa; border-radius: 13px; padding: 12px 14px; box-shadow: 0 10px 28px #536da310; font-size: 12px; }
.device-icon { width: 40px; height: 40px; border-radius: 10px; display: grid; place-items: center; color: #5d89e7; background: #edf4ff; }
.sender { left: -12px; top: 50px; }
.receiver { right: -6px; bottom: 50px; }
.receiver .device-icon { color: #9279d2; background: #f4effc; }
.online-dot { width: 5px; height: 5px; background: #7bb9a0; border-radius: 50%; }
.connection { position: absolute; width: 80px; border-top: 2px dashed #c9d9f8; }
.connection-left { left: 60px; top: 155px; transform: rotate(30deg); }
.connection-right { right: 65px; bottom: 130px; transform: rotate(30deg); }
.floating-label { display: flex; gap: 7px; align-items: center; position: absolute; bottom: -5px; color: #9ba9c2; font-size: 11px; }
.section-caption { display: flex; justify-content: space-between; align-items: baseline; gap: 20px; margin-bottom: 18px; }
h2 { font-size: 17px; font-weight: 650; margin: 0; }
.section-caption > span { color: #9aa4b7; font-size: 12px; }
.entry-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 22px; }
.entry-card { display: flex; align-items: flex-start; position: relative; gap: 20px; padding: 29px; border: 1px solid #e1e8f6; border-radius: 17px; background: linear-gradient(115deg, #fff, #f5f8ff); transition: transform .2s, box-shadow .2s, border-color .2s; }
.entry-card:hover { transform: translateY(-4px); border-color: #b7ccf8; box-shadow: 0 12px 30px #526fa20c; }
.download-entry { background: linear-gradient(115deg, #fff, #f8f6ff); }
.download-entry:hover { border-color: #d4c6f3; }
.entry-icon { flex-shrink: 0; display: grid; place-items: center; width: 54px; height: 54px; border-radius: 14px; background: #eaf1ff; color: #5487ef; }
.download-entry .entry-icon { color: #9576d6; background: #f0eafa; }
.entry-copy { min-width: 0; }
h3 { margin: 1px 0 10px; font-size: 19px; font-weight: 650; }
.entry-copy p { font-size: 12px; color: #8a95a9; margin: 0; line-height: 1.8; }
.entry-cta { display: flex; align-items: center; gap: 8px; color: #5d83cb; font-size: 12px; margin-top: 22px; font-weight: 600; }
.download-entry .entry-cta { color: #9478c8; }
.entry-arrow { color: #a6b6d1; margin-left: auto; flex-shrink: 0; }
.steps { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 35px 10px; margin-top: 10px; }
.step { display: flex; align-items: center; gap: 13px; }
.step-number { align-self: flex-start; font-size: 12px; color: #a3b3d3; padding-top: 2px; font-variant-numeric: tabular-nums; }
.step h3 { font-size: 12px; color: #6b7a94; margin: 0 0 7px; font-weight: 600; }
.step p { font-size: 11px; color: #a0a9ba; margin: 0; line-height: 1.7; }
.step-arrow { color: #c7d2e8; flex-shrink: 0; }
footer { display: flex; justify-content: space-between; gap: 20px; padding: 23px 0; border-top: 1px solid #e4eaf5; color: #9da7ba; font-size: 11px; }
.footer-divider { color: #cbd3e3; margin: 0 8px; }
@media (max-width: 1000px) { .hero { gap: 20px; } .orbit-two { width: 360px; height: 310px; } .sender { left: -10px; top: 25px; } .receiver { right: -5px; bottom: 25px; } .entry-card { padding: 23px; gap: 14px; } .entry-arrow { display: none; } }
@media (max-width: 760px) { .home { padding: 0 24px; } .navigation { height: 80px; } .nav-caption { display: none; } .hero { grid-template-columns: 1fr; padding: 42px 0; gap: 20px; } h1 { font-size: 47px; } .transfer-visual { height: 310px; max-width: 430px; width: 100%; justify-self: center; } .orbit-one { width: 270px; height: 245px; } .orbit-two { width: 320px; height: 290px; } .file-preview { width: 215px; padding: 17px; } .folder-icon { width: 64px; height: 64px; margin: 13px 0; } .sender { left: 0; top: 17px; } .receiver { right: 0; bottom: 16px; } .device { padding: 9px; gap: 7px; font-size: 10px; } .device-icon { width: 32px; height: 32px; } .entry-grid { grid-template-columns: 1fr; gap: 14px; } .entry-card { padding: 24px; } .entry-arrow { display: block; } .section-caption > span { display: none; } .steps { align-items: flex-start; flex-direction: column; padding: 28px 4px; gap: 20px; } .step-arrow { display: none; } footer { flex-direction: column; gap: 9px; } }
@media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
</style>
