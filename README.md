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

HTTP transfers use RAM only on the server. Cached blocks and reserved uploads
share a 256MiB budget (`MAX_CACHE_BYTES`). Each transfer starts with a 2MiB window
(`TRANSFER_MIN_WINDOW_BYTES`) and can borrow up to 32MiB (`TRANSFER_WINDOW_BYTES`),
based on measured receiver acknowledgement throughput. Waiting transfers have
priority for their minimum window; unused capacity is shared rather than divided
equally. Upload admission rotates between transfers and reserves both RAM and
one of four upload slots before the browser sends a multipart body. Unused
permits expire after 30 seconds. At most 128 HTTP transfers may be active
(`MAX_ACTIVE_HTTP_TRANSFERS`); these limits apply to transfer sessions, not accounts.
The browser waits up to two minutes per block for admission, and each verified
receiver acknowledgement releases its block. Cache limits exclude
HTTP buffers, metadata, and runtime overhead, so they are not a process RAM limit.
Stalled transfers expire after 10 minutes of cache inactivity (`BLOCK_TTL_SECS`).
See `.env.example` for configuration. The browser reads the server's chunk size
and file size limit automatically.

The default per-file limit is 10GiB (10240 chunks of 1MiB). Change
`MAX_BLOCKS_PER_FILE` to adjust it; this does not increase the server cache budget.

The receiving browser still holds the file in memory until saving it. Retrying
on the same download page reuses verified chunks; keep that page open because
released server blocks cannot be recovered after a page reload. No server
temporary files are created; browser storage and OS swap behavior are controlled
by the browser and operating system.

Senders can set how long to wait for the receiver on the upload page. The default
is 5 minutes; the supported range is 1–1440 minutes. The countdown includes network
delays and temporary disconnections.

## Docker

```shell
docker compose up -d --build
```

Open `http://server_ip:5000`. The Alpine multi-stage build embeds the frontend in
a stripped musl binary; the final image contains neither Node.js nor Rust build
tools. The container runs as a non-root user with a read-only filesystem and no
data volumes. Compose limits RAM to 512MiB and disables swap. File payloads are
never spooled to server disk; Docker image builds still use normal build storage.
Container logging is disabled by default to avoid persistent log writes.

Copy `.env.example` to `.env` to configure cache limits, STUN/TURN, the host port
(`FILEFLOW_HTTP_PORT`), or the container RAM limit (`FILEFLOW_MEMORY_LIMIT`).
Keep the container RAM limit above the cache budget to leave room for HTTP buffers
and runtime overhead. Recreate the service after changing configuration:

```shell
docker compose up -d --force-recreate
```
