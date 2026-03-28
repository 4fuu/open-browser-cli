class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.5.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.1/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "49af5d736bd68e9c43b4f26c3fb0ef17bd71a69919e0960454c60913d7def906" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.1/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "df1d3d9552cfef900cc45eabebf6c3c412072303f7676d166fefaf50be534cb1" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.1/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "6631b6344035908d046abed4fbff4ad08f10cf4149b383bd7d1ea489a90bcd39" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.1/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6c4871a68c57a2bd7302fb60db7cc6fa77989f90c805ade6a7145444ddeea6b5" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
