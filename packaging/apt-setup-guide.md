# APT Repository Setup Guide for API Faker

This document describes how to prepare API Faker for distribution via APT (Debian/Ubuntu package manager).

## Overview

To distribute via APT, you need to:

1. Create `.deb` packages for each release
2. Host a Debian repository (can be GitHub Pages or any web server)
3. Sign packages with GPG key

## Creating .deb Packages

### Prerequisites

```bash
sudo apt-get install build-essential devscripts debhelper
```

### Debian Package Structure

A `.deb` package needs:

```bash
api-faker_1.2.0_amd64/
├── DEBIAN/
│   └── control
└── usr/
    └── local/
        └── bin/
            └── api-faker
```

### DEBIAN/control file

```bash
Package: api-faker
Version: 1.2.0
Section: utils
Priority: optional
Architecture: amd64
Maintainer: JosunLP <your-email@example.com>
Description: Lightweight Rust application for serving mock HTTP endpoints
 API Faker is a lightweight Rust application that serves HTTP endpoints
 from a JSON configuration file. It is ideal for frontend or integration
 work whenever the real backend is still under construction.
```

### Building the Package

```bash
# Extract binary to package structure
mkdir -p api-faker_1.2.0_amd64/usr/local/bin
mkdir -p api-faker_1.2.0_amd64/DEBIAN

# Copy binary
cp target/release/api-faker api-faker_1.2.0_amd64/usr/local/bin/

# Make it executable
chmod 755 api-faker_1.2.0_amd64/usr/local/bin/api-faker

# Create control file (content shown above)
cat > api-faker_1.2.0_amd64/DEBIAN/control << 'EOF'
Package: api-faker
Version: 1.2.0
Section: utils
Priority: optional
Architecture: amd64
Maintainer: JosunLP <your-email@example.com>
Description: Lightweight Rust application for serving mock HTTP endpoints
 API Faker serves HTTP endpoints from a JSON configuration file.
EOF

# Build the .deb package
dpkg-deb --build api-faker_1.2.0_amd64
```

## Setting Up a Debian Repository

### Option 1: Using GitHub Releases (Simple)

Users can download `.deb` files directly from GitHub releases:

```bash
wget https://github.com/JosunLP/api-faker/releases/download/v1.2.0/api-faker_1.2.0_amd64.deb
sudo dpkg -i api-faker_1.2.0_amd64.deb
```

### Option 2: GitHub Pages Repository (Recommended)

1. Create a new repository for your APT repository
2. Set up the structure:

```bash
apt-repo/
├── dists/
│   └── stable/
│       └── main/
│           └── binary-amd64/
│               ├── Packages
│               └── Packages.gz
└── pool/
    └── main/
        └── a/
            └── api-faker/
                └── api-faker_1.2.0_amd64.deb
```

1. Generate repository metadata:

```bash
# Install required tools
sudo apt-get install dpkg-dev

# Generate Packages file
cd dists/stable/main/binary-amd64/
dpkg-scanpackages ../../../../pool/main > Packages
gzip -k Packages
```

1. Users add your repository:

```bash
# Note: This example uses [trusted=yes] for simplicity but is NOT RECOMMENDED for production
# For production use, properly sign your repository with GPG (see "GPG Signing" section below)
echo "deb https://josunlp.github.io/apt-repo stable main" | sudo tee /etc/apt/sources.list.d/api-faker.list

# If you haven't set up GPG signing, you'll need to add [trusted=yes]
# WARNING: This disables signature verification and should only be used for testing
# echo "deb [trusted=yes] https://josunlp.github.io/apt-repo stable main" | sudo tee /etc/apt/sources.list.d/api-faker.list

sudo apt update
sudo apt install api-faker
```

**Security Note**: Always use proper GPG signing for production repositories (see "GPG Signing" section below). The `[trusted=yes]` option disables signature verification and should only be used for testing purposes.

### Option 3: Using Packagecloud or Gemfury (Hosted)

Commercial services like Packagecloud or Gemfury provide hosted APT repositories.

## Automation

Consider adding a GitHub Action to automatically build `.deb` packages on release:

```yaml
- name: Build .deb package
  run: |
    mkdir -p package/DEBIAN
    mkdir -p package/usr/local/bin

    cp target/release/api-faker package/usr/local/bin/
    chmod 755 package/usr/local/bin/api-faker

    cat > package/DEBIAN/control << 'EOF'
    Package: api-faker
    Version: ${{ github.ref_name }}
    Section: utils
    Priority: optional
    Architecture: amd64
    Maintainer: JosunLP <your-email@example.com>
    Description: Lightweight Rust application for serving mock HTTP endpoints
    EOF

    dpkg-deb --build package
    mv package.deb api-faker_${{ github.ref_name }}_amd64.deb
```

## GPG Signing (Optional but Recommended)

1. Generate GPG key
2. Sign the Release file
3. Distribute public key to users

```bash
# Sign packages
dpkg-sig --sign builder api-faker_1.2.0_amd64.deb

# Users import your key
sudo apt-key adv --keyserver keyserver.ubuntu.com --recv-keys YOUR_KEY_ID
```

## References

- [Debian Binary Package Building HOWTO](https://tldp.org/HOWTO/html_single/Debian-Binary-Package-Building-HOWTO/)
- [Debian Repository Format](https://wiki.debian.org/DebianRepository/Format)
- [GitHub Pages APT Repository](https://assafmo.github.io/2019/05/02/ppa-repo-hosted-on-github.html)
