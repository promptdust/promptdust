# Homebrew cask TEMPLATE for PromptDust (macOS desktop app).
#
# This parameterized template lives in the app repo. It goes live only once a release
# is published and this file is copied into the `homebrew-promptdust` tap.
#
# When pinning a real release in the tapped copy, fill in:
#   - version → the released version (e.g. 0.2.0)
#   - sha256  → sha256 of PromptDust_<version>_universal.dmg, taken from the release's
#               SHA256SUMS (produced by .github/workflows/release.yml). Replace the
#               `:no_check` below with that hex digest so the download is verified.
#
# The build is currently UNSIGNED (macOS Developer ID + notarization land in #41), so a
# first launch may still hit Gatekeeper — see docs/INSTALL.md. `brew install --cask`
# clears the download quarantine, which reduces (does not remove) that friction.
cask "promptdust" do
  version "0.2.0"
  sha256 :no_check # ← replace with the .dmg sha256 when pinning a real release

  url "https://github.com/promptdust/promptdust/releases/download/v#{version}/PromptDust_#{version}_universal.dmg",
      verified: "github.com/promptdust/promptdust/"
  name "PromptDust"
  desc "Read-only map of where AI tools leave data on your machine"
  homepage "https://promptdust.com/"

  app "PromptDust.app"

  # Uninstall cleanup (only runs on `brew uninstall --zap`); missing paths are skipped.
  zap trash: [
    "~/Library/Application Support/com.promptdust.desktop",
    "~/Library/Caches/com.promptdust.desktop",
    "~/Library/Preferences/com.promptdust.desktop.plist",
    "~/Library/Saved Application State/com.promptdust.desktop.savedState",
    "~/Library/WebKit/com.promptdust.desktop",
  ]
end
