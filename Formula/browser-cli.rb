class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.5.4"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.4/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "aec62744c9ae0e810d2d4301d4ce7453baa5334f1b532b7f74bd9c90f2daa426" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.4/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "4f7fbd19eb46e526b04ccd52f5c245ce9cdd3d7088ab16783ff3e11feff24644" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.4/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "6c386d46c35d4278897b2022e7d649797df67af6cdefef840eea7148e19a946f" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.4/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "08edbec0322550ad69e2eaf37fd943b78bf55e644d2fd7c1a7084daae02fbc99" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
