class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.2.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.2.1/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "b51e867eb77119e91cd6c8980ce54ddc4531ec0875159c475fcc301ad2bbcd30" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.2.1/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "1d841713045fb5fce57b84d3360676ff80df56807d8d53ae4e8de267c7266e92" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.2.1/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a0c5438b9429b3b423b08dbf9f2c5144e6038ec7437c606505ada68f6a90a349" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.2.1/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "48ae1970a3de118ab9f37c7d8f22dd6aa27e1912376a7b0ea7f3c08a0ca977f5" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
