class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.6.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.2/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "cf7ca07a1d57cc0ea486beb9f389fc4dea58cf742b16dd134b4c0a3705fc19d0" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.2/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "89f4e7484370dd7d45a49270305aed953bd0969cdad22ee358daf9077cfde24b" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.2/browser-cli-aarch64-unknown-linux-musl.tar.gz"
      sha256 "9a72ac2bf5493fe9851d4b07584ebfb31549e86fc15454286ca4a0d0594644a3" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.2/browser-cli-x86_64-unknown-linux-musl.tar.gz"
      sha256 "1ed93c9e1884ebee0119b892b9ada67d707db4f8172f842a7af08a9c1f2f895d" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
