# FileFlow

[English](README.md) · [简体中文](README.zh-CN.md)

通过浏览器，在设备之间传输文件。

## 安装

### Linux

```sh
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.4.0/FileFlow-linux-x86_64 -O FileFlow
chmod +x FileFlow
./FileFlow
```

### Windows

下载 [FileFlow-windows-x86_64.exe](https://github.com/debugdoctor/FileFlow/releases/download/v0.4.0/FileFlow-windows-x86_64.exe)，然后运行：

```powershell
.\FileFlow-windows-x86_64.exe
```

### Docker

```sh
git clone https://github.com/debugdoctor/FileFlow.git
cd FileFlow
docker compose up -d --build
```

## 使用

打开 `http://localhost:5000`。从其他设备访问时，将 `localhost` 替换为服务器地址。

1. 选择文件，点击「生成链接并上传」。
2. 复制分享链接，发送给接收方。
3. 接收方打开链接，点击「开始下载」，等待文件保存完成。

传输完成前，请保持双方页面打开。
