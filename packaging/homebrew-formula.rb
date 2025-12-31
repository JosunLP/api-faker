# API Faker Homebrew Formula Template
#
# To publish to Homebrew, you can:
# 1. Create a tap repository (recommended): https://github.com/yourusername/homebrew-tap
# 2. Submit to homebrew-core (requires significant usage): https://github.com/Homebrew/homebrew-core
#
# For a personal tap:
# 1. Create a repository named homebrew-tap
# 2. Add this formula to Formula/api-faker.rb
# 3. Users install with: brew install josunlp/tap/api-faker
#
# NOTE: Update version, url, and sha256 for each release
# TODO: Consider automating this with GitHub Actions

class ApiFaker < Formula
  desc "Lightweight Rust application that serves HTTP endpoints from a JSON configuration file"
  homepage "https://github.com/JosunLP/api-faker"
  version "VERSION_PLACEHOLDER"  # TODO: Update to actual version (e.g., 1.2.0) for each release
  license "MIT"

  if OS.mac?
    url "https://github.com/JosunLP/api-faker/releases/download/vVERSION_PLACEHOLDER/api-faker-macos-x86_64.tar.gz"  # TODO: Update VERSION_PLACEHOLDER
    sha256 "SHA256_HASH_PLACEHOLDER"  # TODO: Replace with actual SHA256 from checksums.txt
  elsif OS.linux?
    url "https://github.com/JosunLP/api-faker/releases/download/vVERSION_PLACEHOLDER/api-faker-linux-x86_64.tar.gz"  # TODO: Update VERSION_PLACEHOLDER
    sha256 "SHA256_HASH_PLACEHOLDER"  # TODO: Replace with actual SHA256 from checksums.txt
  end

  def install
    bin.install "api-faker"
  end

  test do
    # Test that the binary runs and shows version
    assert_match version.to_s, shell_output("#{bin}/api-faker --version")
  end
end
