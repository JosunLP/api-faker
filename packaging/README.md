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

Future improvements:
- GitHub Actions workflow to automatically update package manifests
- Automated submission to package manager repositories
- Version bump scripts

## Contributing

If you have experience with package distribution and want to help improve these configurations, please open an issue or PR!
