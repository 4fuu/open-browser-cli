class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.6.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.1/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "ce640422486d5159e4ee4ffa485a736324080a99bb0aa95a064b0dbceb6b8757" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.1/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "a2be577c92071e73af3f8471381a9518a95ad686124df27a60c340643e42a4c0" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.1/browser-cli-aarch64-unknown-linux-musl.tar.gz"
      sha256 "bfac8e46fdc74ed5c4467faf10a1267e162575f80423ebcd0b54b99962bb294d" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.1/browser-cli-x86_64-unknown-linux-musl.tar.gz"
      sha256 "129a6d26d6a9723ae4f7819d1b593e8fe4c7274be799ba39664523e787afa272" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
