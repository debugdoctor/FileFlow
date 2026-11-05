# FileFlow

[English](README.md) · [简体中文](README.zh-CN.md)

通过浏览器，在设备之间传输文件、双向互传文本。

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

1. 在「创建房间」下选择「发送文件」，选择文件，点击「生成链接并上传」。
2. 复制分享链接，发送给接收方。
3. 接收方打开链接，或选择「加入房间 → 接收文件」并输入接收码，点击「开始下载」，等待文件保存完成。

传输完成前，请保持双方页面打开。

### 多人匿名聊天

1. 选择「创建房间 → 创建聊天」，生成聊天房间。
2. 将链接或 5 位房间码（数字与小写字母，与文件接收码格式相同）分享给朋友。其他人打开链接，或选择「加入房间 → 加入聊天」并输入房间码，再点击「加入聊天」。
3. 房间最多 32 人（含房主），每人自动分配随机匿名昵称，房间内不会重名。顶部显示自己的昵称、房间人数和在线人数，消息标明发送者。所有成员均可连续发送文本，并逐条复制。网址可直接点击，单独发送的网址会显示为链接卡片；复制始终保留完整原文。Ctrl / ⌘ + Enter 发送，Enter 换行。

文本经服务器中转并临时保存在内存中，保留最近 100 条 / 256 KiB 记录，单条上限 64 KiB。浏览器允许会话存储时，刷新原标签页可恢复连接及昵称。成员点击「退出房间」可释放名额，其他人继续聊天；关闭标签页或暂时离线仍占用名额。全员离线 1 小时后房间过期；只有房主能结束整个房间，结束或服务重启会清空服务器记录。请妥善保管分享链接。
