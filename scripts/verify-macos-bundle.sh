#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
bundle_root="${1:-$repo_root/src-tauri/target/universal-apple-darwin/release/bundle}"
artifact_root="${2:-$repo_root/artifacts/macos}"
app="$bundle_root/macos/Chronolume.app"
dmg="$(find "$bundle_root/dmg" -maxdepth 1 -type f -name '*.dmg' -print -quit)"

fail() {
  echo "$1" >&2
  exit 1
}

plist_value() {
  local key="$1"
  local plist="$2"
  plutil -extract "$key" raw "$plist" || fail "Info.plist is missing required key: $key"
}

[ -d "$app" ] || fail "macOS app bundle is missing: $app"
[ -n "$dmg" ] || fail "macOS DMG is missing under: $bundle_root/dmg"
[ -f "$app/Contents/Info.plist" ] || fail "Info.plist is missing from the app bundle."
[ -f "$app/Contents/MacOS/chronolume" ] || fail "The Chronolume app executable is missing."

binary="$app/Contents/MacOS/chronolume"
bundled_executables="$(find "$app/Contents/MacOS" -maxdepth 1 -type f -exec basename {} \; | sort)"
[ "$bundled_executables" = "chronolume" ] || fail "Unexpected macOS bundle executables: $bundled_executables"
archs="$(lipo -archs "$binary")"
case " $archs " in *" arm64 "*) ;; *) echo "arm64 slice missing: $archs" >&2; exit 1 ;; esac
case " $archs " in *" x86_64 "*) ;; *) echo "x86_64 slice missing: $archs" >&2; exit 1 ;; esac

bundle_id="$(plist_value CFBundleIdentifier "$app/Contents/Info.plist")"
version="$(plist_value CFBundleShortVersionString "$app/Contents/Info.plist")"
minimum_system="$(plist_value LSMinimumSystemVersion "$app/Contents/Info.plist")"
bundle_name="$(plist_value CFBundleDisplayName "$app/Contents/Info.plist")"
bundle_executable="$(plist_value CFBundleExecutable "$app/Contents/Info.plist")"
[ "$bundle_id" = "com.gaos6e.chronolume" ] || fail "Unexpected CFBundleIdentifier: $bundle_id"
[ "$version" = "2.1.0" ] || fail "Unexpected CFBundleShortVersionString: $version"
[ "$minimum_system" = "12.0" ] || fail "Unexpected LSMinimumSystemVersion: $minimum_system"
[ "$bundle_name" = "Chronolume" ] || fail "Unexpected CFBundleDisplayName: $bundle_name"
[ "$bundle_executable" = "chronolume" ] || fail "Unexpected CFBundleExecutable: $bundle_executable"
if bundle_icon="$(plutil -extract CFBundleIconFile raw "$app/Contents/Info.plist" 2>/dev/null)"; then
  [ -f "$app/Contents/Resources/$bundle_icon" ] || fail "CFBundleIconFile does not resolve inside Contents/Resources: $bundle_icon"
elif bundle_icon="$(plutil -extract CFBundleIconName raw "$app/Contents/Info.plist" 2>/dev/null)"; then
  [ -n "$bundle_icon" ] || fail "CFBundleIconName is empty."
  [ -f "$app/Contents/Resources/Assets.car" ] || fail "CFBundleIconName is set but Resources/Assets.car is missing."
else
  fail "Info.plist has neither CFBundleIconFile nor CFBundleIconName."
fi

if find "$app" -type f \( -name '*.sqlite' -o -name '*.sqlite3' -o -name '*.sqlite-wal' -o -name '*.sqlite-shm' -o -name '*.log' -o -name 'auth.json' \) -print -quit | grep -q .; then
  echo "A database, log, or credential file was bundled into the app." >&2
  exit 1
fi

hdiutil verify "$dmg"
mount_root="$(mktemp -d)"
smoke_home="$(mktemp -d)"
smoke_log="$(mktemp)"
pid=""
cleanup() {
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  hdiutil detach "$mount_root" -quiet 2>/dev/null || true
  rm -rf "$mount_root" "$smoke_home"
  rm -f "$smoke_log"
}
trap cleanup EXIT

hdiutil attach -readonly -nobrowse -mountpoint "$mount_root" "$dmg" >/dev/null
mounted_app="$mount_root/Chronolume.app"
[ -f "$mounted_app/Contents/MacOS/chronolume" ] || fail "Mounted DMG is missing the Chronolume executable."
[ -f "$mounted_app/Contents/Info.plist" ] || fail "Mounted DMG is missing the app Info.plist."
[ ! -e "$smoke_home/.codex" ] || fail "Temporary smoke HOME unexpectedly contains .codex before launch."

HOME="$smoke_home" "$mounted_app/Contents/MacOS/chronolume" >"$smoke_log" 2>&1 &
pid=$!
for _ in $(seq 1 12); do
  if ! kill -0 "$pid" 2>/dev/null; then
    cat "$smoke_log" >&2
    echo "Chronolume exited during the no-~/.codex startup smoke test." >&2
    exit 1
  fi
  sleep 0.25
done
expected_database="$smoke_home/Library/Application Support/Chronolume/v2/chronolume-v2.sqlite3"
[ -f "$expected_database" ] || fail "Launch smoke did not create the expected analytics database: $expected_database"
[ ! -e "$smoke_home/Library/Application Support/com.gaos6e.chronolume" ] || fail "Analytics data was incorrectly nested under the bundle identifier."
kill -TERM "$pid"
for _ in $(seq 1 20); do
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "$pid" 2>/dev/null; then
  kill -KILL "$pid"
fi
wait "$pid" 2>/dev/null || true
pid=""
hdiutil detach "$mount_root" -quiet

rm -rf "$artifact_root"
mkdir -p "$artifact_root"
archive="$artifact_root/Chronolume-${version}-macos-universal.app.zip"
ditto -c -k --sequesterRsrc --keepParent "$app" "$archive"
cp "$dmg" "$artifact_root/"

cat > "$artifact_root/verification-macos.txt" <<EOF
Bundle ID: $bundle_id
Version: $version
Minimum macOS: $minimum_system
Bundle name: $bundle_name
Architectures: $archs
DMG verification: passed
DMG mount: passed
No-~/.codex launch smoke test: passed
Analytics data directory: $expected_database
Bundle-identifier data-directory nesting scan: passed
Bundled database/log/auth.json scan: passed
Signing: candidate workflow uses --no-sign; formal release workflow verifies Developer ID, Gatekeeper, notarization, and stapling separately
EOF

(
  cd "$artifact_root"
  shasum -a 256 "$(basename "$archive")" "$(basename "$dmg")" > SHA256SUMS-macos.txt
)
