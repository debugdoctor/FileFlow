<script setup lang="ts">
import { Button, Typography, Space } from 'ant-design-vue';
import { Upload, Download } from 'lucide-vue-next';

const { Title, Text } = Typography;

const goUpload = () => {
  window.location.href = '/upload';
};

const goDownload = () => {
  window.location.href = '/download';
};
</script>

<template>
  <div class="landing">
    <div class="card">
      <div class="headline">
        <Title :level="2">FileFlow</Title>
        <Text type="secondary">轻量直连，快速传文件</Text>
      </div>
      <div class="actions">
        <div class="pane upload">
          <Space direction="vertical" size="middle">
            <div class="pane-title">
              <Upload :size="28" />
              <span>我要上传</span>
            </div>
            <Text type="secondary">生成链接，与接收方点对点传输</Text>
            <Button type="primary" size="large" block @click="goUpload">进入上传</Button>
          </Space>
        </div>
        <div class="pane download">
          <Space direction="vertical" size="middle">
            <div class="pane-title">
              <Download :size="28" />
              <span>我要下载</span>
            </div>
            <Text type="secondary">输入 5 位 ID，开始获取文件</Text>
            <Button type="default" size="large" block @click="goDownload">进入下载</Button>
          </Space>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.landing {
  min-height: 100vh;
  background: radial-gradient(circle at 20% 20%, #e8f3ff 0%, transparent 25%),
              radial-gradient(circle at 80% 30%, #f3e8ff 0%, transparent 25%),
              linear-gradient(135deg, #f8fbff 0%, #eef2ff 100%);
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 32px;
}

.card {
  width: 100%;
  max-width: 960px;
  background: #ffffff;
  border-radius: 16px;
  padding: 32px;
  box-shadow: 0 20px 80px rgba(15, 23, 42, 0.12);
  border: 1px solid rgba(15, 23, 42, 0.06);
}

.headline {
  text-align: center;
  margin-bottom: 28px;
}

.actions {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 16px;
}

.pane {
  border: 1px solid rgba(15, 23, 42, 0.08);
  border-radius: 12px;
  padding: 20px;
  background: linear-gradient(135deg, rgba(24, 144, 255, 0.04), rgba(24, 144, 255, 0));
}

.pane.download {
  background: linear-gradient(135deg, rgba(82, 196, 26, 0.04), rgba(82, 196, 26, 0));
}

.pane-title {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  font-weight: 600;
  font-size: 16px;
  color: #0f172a;
}
</style>
