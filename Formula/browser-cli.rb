class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/v0.1.0/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/v0.1.0/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/v0.1.0/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/v0.1.0/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
