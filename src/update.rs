use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

const REPO_OWNER: &str = "JosunLP";
const REPO_NAME: &str = "api-faker";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[allow(dead_code)]
    name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Check if a newer version is available
pub async fn check_for_updates() -> Result<Option<String>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("api-faker/{}", CURRENT_VERSION))
        .build()?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        REPO_OWNER, REPO_NAME
    );

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch latest release information")?;

    if !response.status().is_success() {
        bail!(
            "GitHub API request failed with status: {}",
            response.status()
        );
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse release information")?;

    let latest_version = release.tag_name.trim_start_matches('v');
    let current_version = CURRENT_VERSION;

    if version_is_newer(latest_version, current_version) {
        Ok(Some(latest_version.to_string()))
    } else {
        Ok(None)
    }
}

/// Compare version strings using proper semantic versioning
fn version_is_newer(latest: &str, current: &str) -> bool {
    use semver::Version;

    // Parse versions, returning false if either is invalid
    let Ok(latest_ver) = Version::parse(latest) else {
        return false;
    };
    let Ok(current_ver) = Version::parse(current) else {
        return false;
    };

    latest_ver > current_ver
}

/// Perform the update by downloading and replacing the current binary
pub async fn perform_update() -> Result<()> {
    info!("Checking for updates...");

    let client = reqwest::Client::builder()
        .user_agent(format!("api-faker/{}", CURRENT_VERSION))
        .build()?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        REPO_OWNER, REPO_NAME
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        bail!(
            "GitHub API request failed with status: {}",
            response.status()
        );
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("Failed to parse release information from GitHub API")?;

    let latest_version = release.tag_name.trim_start_matches('v');
    let current_version = CURRENT_VERSION;

    if !version_is_newer(latest_version, current_version) {
        info!("Already running the latest version ({})", CURRENT_VERSION);
        return Ok(());
    }

    info!(
        "New version available: {} (current: {})",
        release.tag_name, CURRENT_VERSION
    );

    // Determine the platform-specific archive name
    let archive_name = get_platform_archive_name()?;
    let checksums_name = "checksums.txt";

    // Find the asset URLs
    let archive_asset = release
        .assets
        .iter()
        .find(|a| a.name == archive_name)
        .context(format!("Release asset not found: {}", archive_name))?;

    let checksums_asset = release.assets.iter().find(|a| a.name == checksums_name);

    info!("Downloading {}...", archive_name);
    let archive_data = client
        .get(&archive_asset.browser_download_url)
        .send()
        .await?
        .bytes()
        .await?;

    // Verify checksum if available
    if let Some(checksums_asset) = checksums_asset {
        info!("Downloading checksums...");
        let checksums_data = client
            .get(&checksums_asset.browser_download_url)
            .send()
            .await?
            .text()
            .await?;

        verify_checksum(&archive_data, &checksums_data, &archive_name)?;
    } else {
        warn!("Checksums file not available, skipping verification");
    }

    // Extract and install
    install_update(&archive_data, &archive_name)?;

    info!("Update completed successfully!");
    info!(
        "Please restart api-faker to use version {}",
        release.tag_name
    );

    Ok(())
}

fn get_platform_archive_name() -> Result<String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    // env::consts::OS returns: "linux", "macos", "windows", etc.
    let (platform, ext) = match os {
        "linux" => ("linux", "tar.gz"),
        "macos" => ("macos", "tar.gz"),
        "windows" => ("windows", "zip"),
        _ => bail!("Unsupported platform: {}", os),
    };

    if arch != "x86_64" {
        bail!("Unsupported architecture: {}", arch);
    }

    Ok(format!("api-faker-{}-{}.{}", platform, arch, ext))
}

fn verify_checksum(data: &[u8], checksums: &str, filename: &str) -> Result<()> {
    use std::collections::HashMap;

    // Parse checksums file
    let mut checksums_map = HashMap::new();
    for line in checksums.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            checksums_map.insert(parts[1], parts[0]);
        }
    }

    let expected_hash = checksums_map
        .get(filename)
        .context(format!("Checksum not found for {}", filename))?
        .to_ascii_lowercase();

    // Calculate actual hash
    let actual_hash = hex::encode(sha256_digest(data)).to_ascii_lowercase();

    if actual_hash == expected_hash {
        info!("Checksum verification passed");
        Ok(())
    } else {
        bail!(
            "Checksum verification failed!\nExpected: {}\nGot: {}",
            expected_hash,
            actual_hash
        );
    }
}

fn sha256_digest(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn install_update(archive_data: &[u8], archive_name: &str) -> Result<()> {
    // Get the current executable path
    let current_exe = env::current_exe().context("Failed to get current executable path")?;
    let install_dir = current_exe
        .parent()
        .context("Failed to get executable directory")?;

    info!("Installing to {}...", install_dir.display());

    // Create temporary directory with UUID for security
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};
    let random_suffix = {
        let state = RandomState::new();
        let mut hasher = state.build_hasher();
        std::time::SystemTime::now().hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        std::thread::current().id().hash(&mut hasher);
        hasher.finish()
    };
    let temp_dir = env::temp_dir().join(format!("api-faker-update-{:x}", random_suffix));

    // Create directory, handling unlikely case where it already exists
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    // Ensure cleanup happens even on early returns
    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0); // Ignore cleanup errors
        }
    }
    let _guard = TempDirGuard(temp_dir.clone());

    // Write archive to temp directory
    let archive_path = temp_dir.join(archive_name);
    fs::write(&archive_path, archive_data)?;

    // Extract archive
    extract_archive(&archive_path, &temp_dir)?;

    // Find the binary in extracted files
    let binary_name = if cfg!(windows) {
        "api-faker.exe"
    } else {
        "api-faker"
    };

    let extracted_binary = temp_dir.join(binary_name);
    if !extracted_binary.exists() {
        bail!("Binary not found in archive");
    }

    // Replace current binary
    let target_path = install_dir.join(binary_name);

    // On Windows, we can't replace a running executable directly
    // On Unix, we can replace it and the old one keeps running
    #[cfg(unix)]
    {
        fs::copy(&extracted_binary, &target_path)?;
        // Set executable permissions
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_path, perms)?;
    }

    #[cfg(windows)]
    {
        // On Windows, rename the old binary and copy the new one
        let backup_path = target_path.with_extension("exe.old");

        // Remove old backup if it exists
        if backup_path.exists() {
            fs::remove_file(&backup_path)?;
        }

        // TODO: Consider implementing cleanup of stale .exe.old files on startup
        // to handle cases where the update process is interrupted or crashes.
        // Strategy: On application startup, check for .exe.old files in the
        // installation directory and remove them if they're older than a threshold
        // (e.g., 24 hours) to prevent disk space accumulation over many updates.

        // Rename current to backup, copy new, then clean up backup
        if target_path.exists() {
            fs::rename(&target_path, &backup_path)?;
        }

        match fs::copy(&extracted_binary, &target_path) {
            Ok(_) => {
                // Successfully copied, remove backup
                if backup_path.exists() {
                    let _ = fs::remove_file(&backup_path); // Ignore errors on cleanup
                }
            }
            Err(e) => {
                // Failed to copy, restore backup
                if backup_path.exists() {
                    let _ = fs::rename(&backup_path, &target_path);
                }
                return Err(e.into());
            }
        }
    }

    // Cleanup is handled automatically by TempDirGuard
    Ok(())
}

fn extract_archive(archive_path: &PathBuf, output_dir: &PathBuf) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let filename = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Invalid archive filename")?;

    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        // Handle .tar.gz and .tgz
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(output_dir)?;
    } else if filename.ends_with(".zip") {
        // Handle .zip
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(output_dir)?;
    } else {
        bail!("Unsupported archive format: {}", filename);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_is_newer() {
        assert!(version_is_newer("1.2.1", "1.2.0"));
        assert!(version_is_newer("1.3.0", "1.2.9"));
        assert!(version_is_newer("2.0.0", "1.9.9"));
        assert!(!version_is_newer("1.2.0", "1.2.0"));
        assert!(!version_is_newer("1.2.0", "1.2.1"));
        assert!(!version_is_newer("1.2.9", "1.3.0"));
    }

    #[test]
    fn test_get_platform_archive_name() {
        // Test that the function returns a valid archive name
        let result = get_platform_archive_name();
        assert!(result.is_ok());

        let archive_name = result.unwrap();

        // Should contain platform and architecture
        assert!(archive_name.contains("x86_64"));
        assert!(
            archive_name.contains("linux")
                || archive_name.contains("macos")
                || archive_name.contains("windows")
        );

        // Should have correct extension
        assert!(archive_name.ends_with(".tar.gz") || archive_name.ends_with(".zip"));
    }

    #[test]
    fn test_verify_checksum_valid() {
        let data = b"test data";
        let hash = hex::encode(sha256_digest(data));
        let checksums = format!("{}  test.tar.gz\n", hash);

        let result = verify_checksum(data, &checksums, "test.tar.gz");
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let data = b"test data";
        let checksums = "deadbeef  test.tar.gz\n";

        let result = verify_checksum(data, checksums, "test.tar.gz");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_checksum_missing_file() {
        let data = b"test data";
        let hash = hex::encode(sha256_digest(data));
        let checksums = format!("{}  other.tar.gz\n", hash);

        let result = verify_checksum(data, &checksums, "test.tar.gz");
        assert!(result.is_err());
    }
}
