class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.3.0/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "8d1a32aa997d0ed93566accfb62860e0e8002e918f3ac0a7638cbf31e5da1274" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.3.0/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "98b0a5e59c399cd92eecc5fc5205c9b5f7bf8113958b07d59e22e76076def95f" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.3.0/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "f940f00f12719a0810a5bde266fcaf16abdcb7c7a6b62833d510c5cb00140fb8" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.3.0/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "034eca0d0aa5585798c7f4bf672fad3c3a613e574dcc87641505c579f228571b" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
