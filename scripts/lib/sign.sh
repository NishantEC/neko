# Signing for the Neko app bundles. Sourced by scripts/build-native.sh and
# scripts/build-rune-app.sh so both pick the same identity and sign the same
# way (macOS keeps permissions against the signature, so a Rune bundle signed by
# the certificate that signed the Swift one inherits its grants).
#
#   . "$repo_root/scripts/lib/sign.sh"
#   neko_codesign_bundle <bundle path> <bundle id>
#
# Not run on its own.
neko_codesign_bundle() {
  local bundle="$1" bundle_id="$2"
  # macOS remembers permissions against the signature. An ad-hoc signature is a
  # per-build fingerprint, so every rebuild looked like a new app and asked again.
  # A real certificate keeps the same identity across builds, so permissions stick.
  local identity="${NEKO_CODESIGN_IDENTITY:-}"
  if [[ -z "$identity" ]]; then
    identities="$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/^ *[0-9]*) \([0-9A-F]\{40\}\) "\(Apple Development:.*\)"$/\1 \2/p')"
    # Prefer whichever certificate signed the installed app, so its permissions carry over.
    installed="$(codesign -dvv /Applications/Neko.app 2>&1 | sed -n 's/^Authority=\(Apple Development:.*\)$/\1/p' | head -1 || true)"
    if [[ -n "$installed" ]]; then identity="$(printf '%s\n' "$identities" | awk -v name="$installed" 'substr($0, 42) == name { print $1; exit }')"; fi
    if [[ -z "$identity" ]]; then identity="$(printf '%s\n' "$identities" | head -1 | cut -d' ' -f1)"; fi
    if [[ -z "$identity" ]]; then
      identity=-
      printf 'No Apple Development certificate found; signing ad-hoc. macOS will ask for permissions again after each rebuild.\n' >&2
    fi
  fi
  printf 'Signing with %s\n' "$identity"
  codesign --force --sign "$identity" --identifier "$bundle_id.daemon" --timestamp=none "$bundle/Contents/MacOS/neko-daemon"
  codesign --force --sign "$identity" --identifier "$bundle_id" --timestamp=none "$bundle/Contents/MacOS/neko"
  codesign --force --sign "$identity" --timestamp=none "$bundle"
  codesign --verify --deep --strict "$bundle"
}
