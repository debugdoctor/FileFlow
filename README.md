# FileFlow

[English](README.md) · [简体中文](README.zh-CN.md)

Transfer files between devices, right in your browser.

## Install

### Linux

```sh
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.4.0/FileFlow-linux-x86_64 -O FileFlow
chmod +x FileFlow
./FileFlow
```

### Windows

Download [FileFlow-windows-x86_64.exe](https://github.com/debugdoctor/FileFlow/releases/download/v0.4.0/FileFlow-windows-x86_64.exe), then run:

```powershell
.\FileFlow-windows-x86_64.exe
```

### Docker

```sh
git clone https://github.com/debugdoctor/FileFlow.git
cd FileFlow
docker compose up -d --build
```

## Usage

Open `http://localhost:5000`, or use the server's address when connecting from another device.

1. Choose a file and click “生成链接并上传”.
2. Copy the link and send it to the receiver.
3. Open the link, click “开始下载”, and wait for saving to finish.

Keep both transfer pages open until completion.
