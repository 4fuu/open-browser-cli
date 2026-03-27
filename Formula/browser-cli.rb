class BrowserCli < Formula
  desc "Browser session CLI with Native Messaging relay"
  homepage "https://github.com/4fuu/open-browser-cli"
  version "0.4.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.2/browser-cli-aarch64-apple-darwin.tar.gz"
      sha256 "56c2acdf5513ecf04d886bb5b819529295f24cb3a64bb834500ed185571c15d2" # macos_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.2/browser-cli-x86_64-apple-darwin.tar.gz"
      sha256 "8bdc197fcbcc4552014775483aa9ad0b752b2e7b324c221f4e65eb048f1fe206" # macos_x86
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.2/browser-cli-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "ec46297a40f341c9044f861c75b4db5180f1d7b020bd5857c8e66a3926ca5e1c" # linux_arm64
    end
    on_intel do
      url "https://github.com/4fuu/open-browser-cli/releases/download/0.4.2/browser-cli-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "eda2ab764f43e70bc636b04fa4269413a9dd5a2b5d4521b37c230c5c1a072a45" # linux_x86
    end
  end

  def install
    bin.install "browser-cli"
  end

  test do
    assert_match "browser-cli", shell_output("#{bin}/browser-cli --version")
  end
end
