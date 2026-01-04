# typed: false
# frozen_string_literal: true

# Homebrew Formula for Toki - Automatic time tracking for developers
class Toki < Formula
  desc "Automatic time tracking for developers - Track your work without thinking about it"
  homepage "https://github.com/RikaiDev/toki"
  version "0.2.6"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/RikaiDev/toki/releases/download/v#{version}/toki-cli-aarch64-apple-darwin.tar.xz"
      sha256 "ebcd3af6ed87ac084b621b7b4409bb1b94a6432eba271e66a7417c532774d81b"
    end
    on_intel do
      url "https://github.com/RikaiDev/toki/releases/download/v#{version}/toki-cli-x86_64-apple-darwin.tar.xz"
      sha256 "c6dac2afe5ad087a1c99df2fc867ab206842d98c3467017946432fbb53d40507"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/RikaiDev/toki/releases/download/v#{version}/toki-cli-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "79a374a98d057f7485e93cf7d1cd4cee8801b7c1dfb9ba5c033d604c0c9d3ed8"
    end
    on_intel do
      url "https://github.com/RikaiDev/toki/releases/download/v#{version}/toki-cli-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "13e1e684246d560d3d229c88de547a196f526d85ea6a22544c96748626459983"
    end
  end

  def install
    bin.install "toki"
  end

  def caveats
    on_macos do
      <<~EOS
        To track window titles, you need to grant Accessibility permission:
        System Settings > Privacy & Security > Accessibility > Add your terminal app

        Quick start:
          toki init
          toki start
          toki status
      EOS
    end
  end

  test do
    assert_match "toki #{version}", shell_output("#{bin}/toki --version")
  end
end
