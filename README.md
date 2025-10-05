# FileFlow
Transfer files between two devices via web browser

## Quick Start

### Deployment

```shell
# Linux
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.1.1/FileFlow-linux-x86_64 -O FileFlow

# Windows
wget -c https://github.com/debugdoctor/FileFlow/releases/download/v0.1.1/FileFlow-windows-x86_64.exe -O FileFlow.exe
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
