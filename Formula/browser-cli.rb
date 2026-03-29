class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.6.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.0/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "c6b4542d7d9ae7864f9b02a680bf395fd3e6a5724da959b15ba8df7bfefed1d0" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.0/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "39ea5d9441dee281cba6fdcb05fd0e9d1307c5dfe8d58665bc5dd22f99fcfc48" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.0/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "68a687f683eec026163b91003a3732af4941a104ed08da1993fcde046e456f07" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.6.0/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0fa845db56096ea1453f4b977e6c5cc5a4a9c8bc3fd3453cf846cfe93ead67e1" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
