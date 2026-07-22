# Homebrew formula TEMPLATE for the PromptDust CLI (`promptdust`).
#
# This parameterized template lives in the app repo. It goes live only once a release is
# published and this file is copied into the `homebrew-promptdust` tap as
# `Formula/promptdust.rb` (hence `class Promptdust`, matching that destination filename).
# The tap's sibling desktop cask is
# packaging/homebrew/promptdust.rb.
#
# When pinning a real release in the tapped copy, fill in — from the release's SHA256SUMS
# (produced by .github/workflows/release.yml):
#   - version → bump the `v0.2.0` in each download URL to the released version. Homebrew reads
#               the formula version from the URL, so there is no separate `version` line. (The
#               archive names carry no digits precisely so the `vX.Y.Z` tag is what parses — an
#               "x86_64" in the name would be read as version "86.64"; see package-cli.sh.)
#   - sha256  → replace each placeholder (64 zeros) with the digest of that archive:
#               promptdust-macos-universal.tar.gz for BOTH macOS arch blocks (it is a single
#               universal archive), and promptdust-linux.tar.gz for Linux.
#
# It is a pre-built-binary formula (installs the release archive), not build-from-source:
# each archive holds a single top-level `promptdust` binary. The macOS archive is universal
# (one file serves Apple silicon and Intel); the Linux archive is x86_64 only, so no arm64
# Linux archive is shipped — `brew install` there has no matching download and fails.
class Promptdust < Formula
  desc "Read-only map of where AI tools leave data on your machine"
  homepage "https://promptdust.com/"
  license "Apache-2.0"

  # One universal archive serves both arches, so on_arm and on_intel share the SAME url +
  # sha256. They can't be merged: Homebrew's component rules forbid url/sha256 directly under
  # on_macos, so a per-arch block is required even though the values are identical.
  on_macos do
    on_arm do
      url "https://github.com/promptdust/promptdust/releases/download/v0.2.0/promptdust-macos-universal.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/promptdust/promptdust/releases/download/v0.2.0/promptdust-macos-universal.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/promptdust/promptdust/releases/download/v0.2.0/promptdust-linux.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "promptdust"
  end

  test do
    assert_match "PromptDust #{version}", shell_output("#{bin}/promptdust version")
  end
end
