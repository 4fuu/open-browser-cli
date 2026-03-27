class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.4.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.0/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "4a76ec3b0140e16b2a064c2bb2b360e0617acb5e05f3ad5b5da1229e3e6cc92e" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.0/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "881a0f78007f83b3f8715180bd5e5cce468ec1785ef8011b523d146daf7c7d44" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.0/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "8a71baf508c78087f0b8fb059215413fab578918420c486a29c24c10bd1714fe" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.0/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "1e9305b7f65830cba1f53dff0700678e2381f9209012437d7e57066fd744a0c5" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
