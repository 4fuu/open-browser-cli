class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.5.3"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.3/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "27e981a3b0aca25bee8c13efdbf2393e0437dea5ef934d7be9a396d7ce1a2723" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.3/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "978dbb2c1c64c37c6efc4d9ba63d82765adc3aa78f0b554e9b53f74d67dd9f10" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.3/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "39a4cbf931f25bf446716d65caaae3e35197429cb3e1bcd3b091e73dafcd991e" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.3/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "155ff0efe59404f13f827be1965161d45032b42a1463e5c147b6510ecda84973" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
