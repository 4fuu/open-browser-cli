class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.4.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.1/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "5805d34ba00fb6627d6497a9b90c1d43d841ac1b212ad3e54f92b4dbfe76de95" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.1/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "972e9f9c1194b5adb3961b872e6d4d074a9c87489a6f5c0ba0672bf80f0f0a31" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.1/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "24c467ae8ee97fbd9dbb95278d9320824d2381a54fdcf6d27c7a5fb2fae41d57" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.1/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "75d2eda807f42246ec570fc115161fca4d6f072c01c2026617242aefe934ed23" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
