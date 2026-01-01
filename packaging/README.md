# Package Manager Distribution

This directory contains templates and guides for distributing API Faker through various package managers.

## Available Package Managers

### 1. Winget (Windows Package Manager)

**File:** `winget-manifest.yaml`

Winget is the official package manager for Windows. To submit to winget:

1. Update the version and SHA256 hash in `winget-manifest.yaml`
2. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs)
3. Create a PR with the manifest file in the appropriate directory structure
4. Follow the [winget submission guidelines](https://github.com/microsoft/winget-pkgs/blob/master/CONTRIBUTING.md)

Users can then install with:

```powershell
winget install JosunLP.APIFaker
```

### 2. Homebrew (macOS Package Manager)

**File:** `homebrew-formula.rb`

Homebrew is the most popular package manager for macOS. Options:

#### Option A: Personal Tap (Recommended for starting)

1. Create a repository named `homebrew-tap` in your GitHub account
2. Add `homebrew-formula.rb` to `Formula/api-faker.rb` in that repository
3. Update version and SHA256 hashes

Users install with:

```bash
brew install josunlp/tap/api-faker
```

#### Option B: Homebrew Core (Requires significant usage)

Submit a PR to [Homebrew/homebrew-core](https://github.com/Homebrew/homebrew-core) once the project gains traction.

### 3. APT (Debian/Ubuntu Package Manager)

**File:** `apt-setup-guide.md`

APT is used by Debian and Ubuntu-based distributions. The guide covers:

- Creating `.deb` packages
- Setting up a Debian repository (multiple options)
- Automation strategies
- GPG signing

## Updating Package Definitions

When releasing a new version:

1. **Winget**: Update `PackageVersion`, `InstallerUrl`, and `InstallerSha256`
2. **Homebrew**: Update `version`, `url`, and `sha256` for each platform
3. **APT**: Follow the guide to build new `.deb` packages

## Getting SHA256 Hashes

SHA256 hashes are automatically generated during the release process and included in `checksums.txt` in each release.

Alternatively, calculate manually:

```bash
# Linux/macOS
sha256sum api-faker-linux-x86_64.tar.gz

# Windows (PowerShell)
Get-FileHash -Path api-faker-windows-x86_64.zip -Algorithm SHA256
```

## Automation

The release workflow (`.github/workflows/release.yml`) automatically:

1. **Generates package manifests** with correct versions and SHA256 hashes:

   - `winget-manifest.yaml` - Ready for Winget submission
   - `homebrew-formula.rb` - Ready for Homebrew tap

2. **Builds Debian packages** (`.deb`) for apt-based distributions

3. **Uploads all artifacts** to the GitHub Release:

   - Platform binaries (Linux, macOS, Windows)
   - Checksums file
   - Install script
   - Generated package manifests
   - Debian package with checksum

4. **Publishes to package managers** (when enabled):
   - **Winget**: Automatically submits to microsoft/winget-pkgs
   - **Homebrew**: Updates the homebrew-tap repository
   - **APT**: Publishes repository to GitHub Pages

### Enabling automated publishing

To enable automatic publishing to package managers, configure the following repository settings:

#### Winget (Windows Package Manager)

1. Create a Personal Access Token (PAT) with `public_repo` scope
2. Add the token as repository secret: `WINGET_TOKEN`
3. Add repository variable: `ENABLE_WINGET_PUBLISH` = `true`

> **Note**: The package must be manually submitted to Winget at least once before automated updates work.

#### Homebrew

1. Create a repository named `homebrew-tap` in your GitHub account
2. Generate an SSH deploy key for the tap repository
3. Add the private key as repository secret: `HOMEBREW_TAP_DEPLOY_KEY`
4. Add repository variable: `ENABLE_HOMEBREW_PUBLISH` = `true`

#### APT Repository (GitHub Pages)

1. Enable GitHub Pages in repository settings (source: GitHub Actions)
2. Add repository variable: `ENABLE_APT_PUBLISH` = `true`

Users can then install with:

```bash
# Add repository (uses trusted=yes for simplicity)
echo "deb [trusted=yes] https://josunlp.github.io/api-faker stable main" | sudo tee /etc/apt/sources.list.d/api-faker.list
sudo apt update
sudo apt install api-faker
```

### Manual publishing (fallback)

If automated publishing is not enabled, download the generated manifests from release assets:

**Winget:**

```bash
# Download the generated manifest
curl -LO https://github.com/JosunLP/api-faker/releases/download/v1.2.0/winget-manifest.yaml

# Submit to winget-pkgs repository manually
# See: https://github.com/microsoft/winget-pkgs/blob/master/CONTRIBUTING.md
```

**Homebrew:**

```bash
# For your personal tap, copy the formula to your homebrew-tap repository
curl -LO https://github.com/JosunLP/api-faker/releases/download/v1.2.0/homebrew-formula.rb
# Copy to: homebrew-tap/Formula/api-faker.rb
```

**APT/Debian:**

```bash
# Download and install the .deb package directly
curl -LO https://github.com/JosunLP/api-faker/releases/download/v1.2.0/api-faker_1.2.0_amd64.deb
sudo dpkg -i api-faker_1.2.0_amd64.deb
```

## Contributing

If you have experience with package distribution and want to help improve these configurations, please open an issue or PR!
