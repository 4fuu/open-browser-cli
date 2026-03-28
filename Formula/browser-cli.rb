class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.5.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.0/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "b881649c2ca3c68f1bb670955be8bfc9b0a3bf2abf8cd0f6a1f65431bfb5337b" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.0/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "ebdcc10e0c6fc2c90ea544dad482162060f5f9085c31688f7d1554653e4dea99" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.0/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "8bcd513c68d02b750040064c8372891af71a98d4c46d1f045c3e7fdd86cf42be" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.5.0/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0a11710a3b19a22787de246ed4ef186628bf3f2cd594b651682715f48d391ae4" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
