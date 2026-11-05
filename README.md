# FileFlow

[English](README.md) · [简体中文](README.zh-CN.md)

Transfer files and exchange text between devices, right in your browser.

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

1. Under **创建房间**, choose **发送文件**, select a file and click “生成链接并上传”.
2. Copy the link and send it to the receiver.
3. Open the link (or choose **加入房间 → 接收文件** and enter the code), click “开始下载”, and wait for saving to finish.

Keep both transfer pages open until completion.

### Anonymous group chat

1. Choose **创建房间 → 创建聊天** to create a chat room.
2. Share the link or 5-character room code (digits and lowercase letters, matching file transfer codes) with friends. Open the link or choose **加入房间 → 加入聊天**, then click **加入聊天**.
3. Up to 32 people, including the host, can join a room. Each receives a random anonymous nickname, unique within the room. The room shows your nickname, member and online counts, and the sender of each message. Everyone can send text repeatedly and copy individual messages. URLs are clickable, and standalone URLs appear as link cards; copying preserves the complete original text. Use Ctrl/Cmd + Enter to send; Enter inserts a newline.

Text is relayed through the server and kept in memory (up to 100 messages / 256 KiB per room, 64 KiB per message). Refreshing the same tab restores its credentials and nickname when session storage is available. Members can click **退出房间** to free a place while others continue chatting; closing a tab or going offline still occupies a place. Rooms expire after everyone has been inactive for one hour. Only the host can end the entire room; ending it or restarting the server clears its history. Keep the room link private.
