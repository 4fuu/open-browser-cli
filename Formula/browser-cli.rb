class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.4.3"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.3/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "ab19b274af11208f4d7a91893f71158e0abe1419a1d9a69176598be74381883c" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.3/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "8a332b794103b14c3ac3a89cc0054ceaba17668ae916664800a72d6078fccd7c" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.3/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "45d96cfe38fb6ccfce9f86e153bd78825ef7e19f6f253c699de0f9544483d4df" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.3/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "d9c2b27de427646364322c98f8480f242c1170d2d5617e7a9509834f8ea0444f" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
