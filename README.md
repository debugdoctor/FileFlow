# FileFlow
Transfer files between two devices via web browser

## Quick Start

### Deployment

```shell
# Linux
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.2.0/FileFlow-linux-x86_64 -O FileFlow

# Windows
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.2.0/FileFlow-windows-x86_64.exe -O FileFlow.exe
```

### Usage
Use the following command to start FileFlow serrver

```shell
# Linux
./FileFlow > FIleFlow.log 2>&1 & echo $! > FileFlow.pid

# Windows
start /b FileFlow.exe > FileFlow.log 2>&1
```

Open your browser and visit `http://server_ip:5000/upload` then you can upload files and follow the instructions to download files.

## Memory usage

HTTP transfers use RAM only on the server. By default, cached file payloads are
limited to 256MiB globally (`MAX_CACHE_BYTES`) and a 32MiB sliding window per
transfer (`TRANSFER_WINDOW_BYTES`). Uploads wait when either limit is reached;
each verified receiver acknowledgement releases its block. Cache limits exclude
HTTP buffers, metadata, and runtime overhead, so they are not a process RAM limit.
Stalled transfers expire after 10 minutes of cache inactivity (`BLOCK_TTL_SECS`).
See `.env.example` for configuration. The browser reads the server's chunk size
and file size limit automatically.

The receiving browser still holds the file in memory until saving it. Retrying
on the same download page reuses verified chunks; keep that page open because
released server blocks cannot be recovered after a page reload. No server
temporary files are created; browser storage and OS swap behavior are controlled
by the browser and operating system.
