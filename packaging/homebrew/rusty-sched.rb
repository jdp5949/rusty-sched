# Homebrew Formula for rusty-sched.
#
# This file is the template kept inside the rusty-sched repo. To actually
# publish a new version:
#
#   1. After the GitHub Release for vX.Y.Z is live, fetch the macOS tarball
#      SHA256s:
#        curl -sL https://github.com/jdp5949/rusty-sched/releases/download/vX.Y.Z/\
#          rusty-sched-vX.Y.Z-aarch64-apple-darwin.tar.gz | shasum -a 256
#        curl -sL https://github.com/jdp5949/rusty-sched/releases/download/vX.Y.Z/\
#          rusty-sched-vX.Y.Z-x86_64-apple-darwin.tar.gz | shasum -a 256
#   2. Bump `version`, both `sha256` values, and the URL `vX.Y.Z` in this file.
#   3. Copy this file to the separate tap repo `jdp5949/homebrew-rusty-sched`
#      at `Formula/rusty-sched.rb`, commit, and push to its `main` branch.
#   4. Users install via:  `brew install jdp5949/rusty-sched/rusty-sched`
#
class RustySched < Formula
  desc "Job scheduler with retries, triggers, and a built-in agent"
  homepage "https://github.com/jdp5949/rusty-sched"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/jdp5949/rusty-sched/releases/download/v#{version}/rusty-sched-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/jdp5949/rusty-sched/releases/download/v#{version}/rusty-sched-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    subdir = "rusty-sched-v#{version}-#{Hardware::CPU.arch == :arm64 ? "aarch64" : "x86_64"}-apple-darwin"
    bin.install "#{subdir}/rusty-sched"
  end

  test do
    assert_match(/rusty-sched/, shell_output("#{bin}/rusty-sched --version"))
  end
end
