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

Choose a file and click “生成链接并上传”, then copy the generated download link
and send it to the receiver. Opening the link goes straight to that file's download
page without entering a code. Keep the sending page open during the transfer.

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

On trusted HTTPS in Chrome / Edge, the receiver can choose “边下载边保存” and a
save location before transfer starts. This mode uses bounded HTTP download workers,
serializes disk writes, and acknowledges each block only after writing it. Network
retries reuse written offsets without retaining the whole payload. Other browsers
and HTTP use the compatible in-memory download mode. Both ends show a three-second
moving transfer rate; upload rate counts accepted/queued bytes, not remote disk speed.
Keep the download page open because released server blocks cannot be recovered
after a page reload. Browser disk write failures require a new transfer link.
No server
temporary files are created; browser storage and OS swap behavior are controlled
by the browser and operating system.
The browser's file API may stage writes in its own temporary file until closing;
streaming reduces application RAM use but does not promise exactly one physical
disk write per byte.

Senders can set how long to wait for the receiver on the upload page. The default
is 5 minutes; the supported range is 1–1440 minutes. The countdown includes network
delays and temporary disconnections.
The collapsed advanced settings contain three controls: HTTP chunk size (default
1MiB, bounded by server configuration), upload concurrency (1/2/4/8 workers,
default 4), and receiver wait time. Chunk size is recorded in transfer metadata so
the receiver uses the same ranges. Smaller chunks do not lower the file size cap;
global server admission and RAM limits still apply. P2P uses its own transport chunks.

Both transfer pages provide a stop button. Stopping cancels active requests,
retry waits, and P2P connections. A stopped streaming download aborts its unfinished
file write; start a new transfer link to download again because acknowledged blocks
may already have been released from the server.

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

### LAN HTTPS

Generate a private LAN CA and an IP certificate on the server:

```sh
sh deploy/create-lan-cert.sh 10.23.58.43
docker run --rm --user 0:0 --entrypoint sh -v "$PWD/deploy/tls:/tls" alpine:3.22 \
  -c 'chown 101:101 /tls/server.key && chmod 400 /tls/server.key'
docker compose -p fileflow -f docker-compose.yml -f docker-compose.https.yml up -d --build
```

Use `https://10.23.58.43`. Import `deploy/tls/ca.crt` into each receiving device's
trusted root store, verifying its SHA-256 fingerprint against the server output.
The public CA certificate is also available at `/fileflow-ca.crt`; private keys
stay on the server and are excluded from Git and image builds. Merely ignoring a
browser certificate warning does not establish a trusted secure context.
The HTTPS proxy disables request/response buffering and uses RAM for temporary
storage. Certificates must be renewed before expiry (server certificate: one year).
