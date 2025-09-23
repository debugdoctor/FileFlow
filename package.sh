#!/bin/bash

# package.sh - Build and package FileFlow for all supported platforms
# Usage: ./package.sh [version]

set -e  # Exit on any error

VERSION=${1:-"unknown"}

# Define variables
PROJECT_NAME="FileFlow"
BUILD_DIR="build"
DIST_DIR="web/dist"

# Function to check if Node.js is installed
check_node() {
    if command -v node >/dev/null 2>&1; then
        NODE_VERSION=$(node --version)
        echo "Node.js is already installed: $NODE_VERSION"
        return 0
    else
        echo "Node.js is not installed"
        return 1
    fi
}

# Function to install Node.js 24
install_node() {
    echo "Installing Node.js 24..."
    
    # Detect OS
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Install using NodeSource repository for Ubuntu/Debian
        curl -fsSL https://deb.nodesource.com/setup_24.x | sudo -E bash - 
        sudo apt-get install -y nodejs
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # On macOS, use Homebrew
        if ! command -v brew >/dev/null 2>&1; then
            echo "Homebrew is not installed. Please install Homebrew first: https://brew.sh/"
            exit 1
        fi
        brew install node@24
    else
        echo "Unsupported OS. Please install Node.js 24 manually."
        exit 1
    fi
    
    # Verify installation
    if command -v node >/dev/null 2>&1; then
        NODE_VERSION=$(node --version)
        echo "Node.js successfully installed: $NODE_VERSION"
    else
        echo "Failed to install Node.js"
        exit 1
    fi
}

# Function to check if pnpm is installed
check_pnpm() {
    if command -v pnpm >/dev/null 2>&1; then
        PNPM_VERSION=$(pnpm --version)
        echo "pnpm is already installed: $PNPM_VERSION"
        return 0
    else
        echo "pnpm is not installed"
        return 1
    fi
}

# Function to install pnpm
install_pnpm() {
    echo "Installing pnpm..."
    npm install -g pnpm
    
    if command -v pnpm >/dev/null 2>&1; then
        PNPM_VERSION=$(pnpm --version)
        echo "pnpm successfully installed: $PNPM_VERSION"
    else
        echo "Failed to install pnpm"
        exit 1
    fi
}

# Function to check if Rust is installed
check_rust() {
    if command -v rustc >/dev/null 2>&1; then
        RUST_VERSION=$(rustc --version)
        echo "Rust is already installed: $RUST_VERSION"
        return 0
    else
        echo "Rust is not installed"
        return 1
    fi
}

# Function to install Rust
install_rust() {
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    
    # Source cargo environment
    source "$HOME/.cargo/env"
    
    if command -v rustc >/dev/null 2>&1; then
        RUST_VERSION=$(rustc --version)
        echo "Rust successfully installed: $RUST_VERSION"
    else
        echo "Failed to install Rust"
        exit 1
    fi
}

# Function to check if mingw-w64 is installed (needed for Windows cross-compilation)
check_mingw_w64() {
    if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
        echo "mingw-w64 is already installed"
        return 0
    else
        echo "mingw-w64 is not installed"
        return 1
    fi
}

# Function to install mingw-w64
install_mingw_w64() {
    echo "Installing mingw-w64..."
    
    # Detect OS
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Install mingw-w64 for Ubuntu/Debian
        sudo apt-get update
        sudo apt-get install -y mingw-w64
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # On macOS, use Homebrew
        if ! command -v brew >/dev/null 2>&1; then
            echo "Homebrew is not installed. Please install Homebrew first: https://brew.sh/"
            exit 1
        fi
        brew install mingw-w64
    else
        echo "Unsupported OS. Please install mingw-w64 manually."
        exit 1
    fi
    
    # Verify installation
    if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
        echo "mingw-w64 successfully installed"
    else
        echo "Failed to install mingw-w64"
        exit 1
    fi
}

# Check and install Node.js if needed
echo "Checking Node.js installation..."
if ! check_node; then
    install_node
fi

# Check and install pnpm if needed
echo "Checking pnpm installation..."
if ! check_pnpm; then
    install_pnpm
fi

# Check and install Rust if needed
echo "Checking Rust installation..."
if ! check_rust; then
    install_rust
    # Source cargo environment for current session
    source "$HOME/.cargo/env"
fi

# Check and install mingw-w64 if needed (for Windows cross-compilation)
echo "Checking mingw-w64 installation (needed for Windows cross-compilation)..."
if ! check_mingw_w64; then
    install_mingw_w64
fi

# Create build directory
echo "Creating build directory..."
mkdir -p "${BUILD_DIR}"

# Build frontend first
echo "Building frontend..."
cd web
pnpm install
pnpm build-only
cd ..

# Function to package for a specific platform
package_for_platform() {
    local target=$1
    local platform_name=$2
    local output_name="${PROJECT_NAME}-${platform_name}"
    
    echo "Packaging for ${platform_name} (${target})..."
    
    # Use cross for cross-compilation
    cd server
    rustup target add "${target}" 2>/dev/null || true
    cargo build --release --target "${target}"
    cd ..
    
    # Copy binary directly to build directory with platform-specific name
    if [[ "${platform_name}" == *"windows"* ]]; then
        cp "server/target/${target}/release/FileFlow.exe" "${BUILD_DIR}/${output_name}.exe" 2>/dev/null || \
        cp "server/target/${target}/release/file_flow.exe" "${BUILD_DIR}/${output_name}.exe" 2>/dev/null || \
        cp "server/target/${target}/release/FileFlow" "${BUILD_DIR}/${output_name}.exe"
    else
        cp "server/target/${target}/release/FileFlow" "${BUILD_DIR}/${output_name}" 2>/dev/null || \
        cp "server/target/${target}/release/file_flow" "${BUILD_DIR}/${output_name}"
    fi
    
    echo "Created ${BUILD_DIR}/${output_name}"
}

# Package for all platforms
echo "Starting packaging process for version: ${VERSION}"

# Linux packaging
package_for_platform "x86_64-unknown-linux-musl" "linux-x86_64"

# Windows packaging
package_for_platform "x86_64-pc-windows-gnu" "windows-x86_64"

echo "All packages created successfully in ${BUILD_DIR}/"