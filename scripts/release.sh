#!/usr/bin/env bash
#
# Cut a Stash release: build a signed universal .app, generate the updater
# manifest (latest.json), and publish a GitHub Release. Installed copies of
# Stash auto-detect it via the endpoint in tauri.conf.json.
#
# Usage:
#   scripts/release.sh ["release notes"]
#
# Prereqs:
#   - signing key at ~/.tauri/stash.key  (NEVER commit this)
#   - gh CLI logged in with repo scope
#   - bump "version" in src-tauri/tauri.conf.json before running
#
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

REPO="isobutane101/stash"
KEY="$HOME/.tauri/stash.key"
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
TAG="v$VERSION"
NOTES="${1:-Stash $VERSION}"

[ -f "$KEY" ] || { echo "✗ Missing signing key at $KEY"; exit 1; }
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

echo "▶ Building signed universal bundle for $TAG …"
npm run tauri build -- --target universal-apple-darwin

BUNDLE="src-tauri/target/universal-apple-darwin/release/bundle"
DMG="$(ls "$BUNDLE"/dmg/*.dmg | head -1)"
TARGZ="$(ls "$BUNDLE"/macos/*.app.tar.gz | head -1)"
SIG="$(ls "$BUNDLE"/macos/*.app.tar.gz.sig | head -1)"
URL="https://github.com/$REPO/releases/download/$TAG/Stash_universal.app.tar.gz"

# Stable, friendly asset names.
cp "$DMG" /tmp/Stash_universal.dmg
cp "$TARGZ" /tmp/Stash_universal.app.tar.gz

# Build latest.json safely (node handles JSON escaping). Both macOS arches point
# at the one universal tarball.
node -e '
  const fs=require("fs");
  const [sigPath,version,notes,url]=process.argv.slice(1);
  const signature=fs.readFileSync(sigPath,"utf8").trim();
  const o={version,notes,pub_date:new Date().toISOString(),
    platforms:{"darwin-aarch64":{signature,url},"darwin-x86_64":{signature,url}}};
  fs.writeFileSync("/tmp/latest.json",JSON.stringify(o,null,2));
' "$SIG" "$VERSION" "$NOTES" "$URL"

echo "▶ Publishing GitHub Release $TAG …"
gh release create "$TAG" --repo "$REPO" --title "Stash $VERSION" --notes "$NOTES" \
  /tmp/Stash_universal.dmg \
  /tmp/Stash_universal.app.tar.gz \
  /tmp/latest.json

echo "✓ Released: https://github.com/$REPO/releases/tag/$TAG"
