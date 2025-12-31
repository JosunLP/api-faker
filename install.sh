#!/bin/sh
# API Faker Installation Script
# Works on Linux, macOS, and Windows (Git Bash/WSL)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="JosunLP/api-faker"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
GITHUB_API="https://api.github.com/repos/${REPO}"

# Utility functions
log_info() {
    printf "${GREEN}[INFO]${NC} %s\n" "$1"
}

log_warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$1"
}

log_error() {
    printf "${RED}[ERROR]${NC} %s\n" "$1"
}

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"
    
    case "$OS" in
        Linux*)
            PLATFORM="linux"
            BINARY_NAME="api-faker"
            ARCHIVE_EXT="tar.gz"
            ;;
        Darwin*)
            PLATFORM="macos"
            BINARY_NAME="api-faker"
            ARCHIVE_EXT="tar.gz"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            PLATFORM="windows"
            BINARY_NAME="api-faker.exe"
            ARCHIVE_EXT="zip"
            ;;
        *)
            log_error "Unsupported operating system: $OS"
            exit 1
            ;;
    esac
    
    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        arm64|aarch64)
            log_error "ARM64 architecture is not yet supported"
            exit 1
            ;;
        *)
            log_error "Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac
    
    ARCHIVE_NAME="api-faker-${PLATFORM}-${ARCH}.${ARCHIVE_EXT}"
    log_info "Detected platform: ${PLATFORM}-${ARCH}"
}

# Get latest release information from GitHub
get_latest_release() {
    log_info "Fetching latest release information..."
    
    if command -v curl >/dev/null 2>&1; then
        RELEASE_INFO=$(curl -sL "${GITHUB_API}/releases/latest")
    elif command -v wget >/dev/null 2>&1; then
        RELEASE_INFO=$(wget -qO- "${GITHUB_API}/releases/latest")
    else
        log_error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi
    
    # Extract version tag
    VERSION=$(echo "$RELEASE_INFO" | grep '"tag_name"' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/')
    
    if [ -z "$VERSION" ]; then
        log_error "Failed to fetch latest release information"
        exit 1
    fi
    
    log_info "Latest version: $VERSION"
}

# Download file
download_file() {
    URL="$1"
    OUTPUT="$2"
    
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" -o "$OUTPUT" || return 1
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$URL" -O "$OUTPUT" || return 1
    else
        log_error "Neither curl nor wget found"
        exit 1
    fi
}

# Download and verify release
download_release() {
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"
    HASH_URL="https://github.com/${REPO}/releases/download/${VERSION}/checksums.txt"
    
    TMP_DIR=$(mktemp -d -t 'api-faker.XXXXXX' 2>/dev/null || mktemp -d 2>/dev/null)
    if [ -z "$TMP_DIR" ] || [ ! -d "$TMP_DIR" ]; then
        log_error "Failed to create temporary directory"
        exit 1
    fi
    trap 'rm -rf "$TMP_DIR"' EXIT
    
    log_info "Downloading $ARCHIVE_NAME..."
    if ! download_file "$DOWNLOAD_URL" "$TMP_DIR/$ARCHIVE_NAME"; then
        log_error "Failed to download $ARCHIVE_NAME"
        exit 1
    fi
    
    # Download and verify checksums if available
    log_info "Downloading checksums..."
    if download_file "$HASH_URL" "$TMP_DIR/checksums.txt" 2>/dev/null; then
        log_info "Verifying checksum..."
        
        cd "$TMP_DIR"
        
        # Extract the hash for our specific file
        EXPECTED_HASH=$(grep "$ARCHIVE_NAME" checksums.txt | awk '{print $1}')
        
        if [ -z "$EXPECTED_HASH" ]; then
            log_warn "No checksum found for $ARCHIVE_NAME, skipping verification"
        else
            # Calculate actual hash
            if command -v sha256sum >/dev/null 2>&1; then
                ACTUAL_HASH=$(sha256sum "$ARCHIVE_NAME" | awk '{print $1}')
            elif command -v shasum >/dev/null 2>&1; then
                ACTUAL_HASH=$(shasum -a 256 "$ARCHIVE_NAME" | awk '{print $1}')
            else
                log_warn "sha256sum/shasum not found, skipping verification"
                ACTUAL_HASH=""
            fi
            
            if [ -n "$ACTUAL_HASH" ]; then
                if [ "$EXPECTED_HASH" = "$ACTUAL_HASH" ]; then
                    log_info "Checksum verification passed"
                else
                    log_error "Checksum verification failed!"
                    log_error "Expected: $EXPECTED_HASH"
                    log_error "Got: $ACTUAL_HASH"
                    exit 1
                fi
            fi
        fi
        
        cd - >/dev/null
    else
        log_warn "Checksums file not available, skipping verification"
    fi
    
    # Extract archive
    log_info "Extracting archive..."
    cd "$TMP_DIR"
    
    if [ "$ARCHIVE_EXT" = "tar.gz" ]; then
        tar -xzf "$ARCHIVE_NAME"
    elif [ "$ARCHIVE_EXT" = "zip" ]; then
        if command -v unzip >/dev/null 2>&1; then
            unzip -q "$ARCHIVE_NAME"
        else
            log_error "unzip command not found"
            exit 1
        fi
    fi
    
    cd - >/dev/null
    
    BINARY_PATH="$TMP_DIR/$BINARY_NAME"
}

# Install binary
install_binary() {
    log_info "Installing to $INSTALL_DIR..."
    
    # Determine the target filename (preserve .exe on Windows)
    TARGET_NAME="api-faker"
    if [ "$PLATFORM" = "windows" ]; then
        TARGET_NAME="api-faker.exe"
    fi
    
    # Check if we need sudo
    if [ ! -w "$INSTALL_DIR" ]; then
        if command -v sudo >/dev/null 2>&1; then
            log_warn "Root privileges required for installation to $INSTALL_DIR"
            sudo mkdir -p "$INSTALL_DIR"
            sudo cp "$BINARY_PATH" "$INSTALL_DIR/$TARGET_NAME"
            sudo chmod +x "$INSTALL_DIR/$TARGET_NAME"
        else
            log_error "Cannot write to $INSTALL_DIR and sudo not available"
            log_info "Try setting INSTALL_DIR to a writable location:"
            log_info "  export INSTALL_DIR=~/.local/bin"
            log_info "  Then run the install script again"
            exit 1
        fi
    else
        mkdir -p "$INSTALL_DIR"
        cp "$BINARY_PATH" "$INSTALL_DIR/$TARGET_NAME"
        chmod +x "$INSTALL_DIR/$TARGET_NAME"
    fi
    
    log_info "Installation complete!"
    log_info ""
    log_info "api-faker $VERSION has been installed to $INSTALL_DIR/$TARGET_NAME"
    log_info ""
    log_info "Make sure $INSTALL_DIR is in your PATH."
    log_info "You can now run: api-faker --help"
}

# Main installation flow
main() {
    log_info "API Faker Installation Script"
    log_info ""
    
    detect_platform
    get_latest_release
    download_release
    install_binary
    
    # Note about unsigned software
    if [ "$PLATFORM" = "macos" ]; then
        log_info ""
        log_warn "Note for macOS users:"
        log_info "This binary is not signed. On first run, you may need to:"
        log_info "1. Go to System Preferences > Security & Privacy"
        log_info "2. Click 'Allow Anyway' for api-faker"
        log_info "Or run: xattr -d com.apple.quarantine $INSTALL_DIR/api-faker"
    fi
}

main "$@"
