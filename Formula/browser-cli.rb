class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.5.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.2/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "5da0459b12bcf66f322cf1185b64bfc08ab73c784ac691b232d35ce668d183b0" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.2/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "2170010c237231f8a291972c9e8a9a289ed07d9d78c53cf09c722d654ef03939" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.2/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "79e4845ff04088a546126154037a06b0baab674866cd77d7b7dd84874d622080" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.2/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "23f4edc354d5b1113ac1abdec0a3362a22e800107e63936c9ddd658dc540bb85" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
