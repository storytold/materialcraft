#!/usr/bin/env bash
# Validate the staged Arch launcher without building or opening MaterialCraft.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
stage="$work/source"
dst="$work/dist"
base=materialcraft-0.0.1-nightly.1-linux-x86_64
mkdir -p "$stage/bin" "$dst" "$work/extract"

# A local argv recorder stands in for the compiled binaries. It never opens a UI.
cat >"$stage/bin/materialcraft" <<'RECORDER'
#!/bin/sh
printf '<%s>\n' "$@"
RECORDER
chmod 755 "$stage/bin/materialcraft"
cp "$stage/bin/materialcraft" "$stage/bin/materialcraft-cli"

bash "$HERE/nightly-arch-tar.sh" "$stage" "$work/stage" "$dst" "$base"
tar -xzf "$dst/$base.tar.gz" -C "$work/extract"
bin="$work/extract/$base/bin"
# Simulate the existing Arch bin-tree copy and public launcher rename.
mkdir -p "$work/installed"
cp -R "$bin" "$work/installed/bin"
bin="$work/installed/bin"
mv "$bin/materialcraft" "$bin/materialcraft-nightly"
mv "$bin/materialcraft-cli" "$bin/materialcraft-nightly-cli"

test ! -e "$stage/bin/.materialcraft-nightly"
cmp "$stage/bin/materialcraft" "$bin/.materialcraft-nightly/materialcraft"
cmp "$stage/bin/materialcraft-cli" "$bin/materialcraft-nightly-cli"
expect() {
  expected="$1"
  shift
  actual=$(PATH="$bin:$PATH" materialcraft-nightly "$@")
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected:\n%s\nactual:\n%s\n' "$expected" "$actual" >&2
    exit 1
  fi
}
# The launcher names the profile the flavor's way and keeps every argument after the delimiter.
launch() {
  local profile="$1"
  shift
  printf '<--launch-profile>\n<nightly-v1>\n<--profile-root>\n<%s>\n<-->\n' "$profile"
  for file in "$@"; do printf '<%s>\n' "$file"; done
}
export XDG_DATA_HOME="$work/xdg with space"
expect "$(launch "$XDG_DATA_HOME/materialcraft-nightly")"
expect "$(launch "$XDG_DATA_HOME/materialcraft-nightly" 'material with spaces.mcraft' '--other-name.mcraft')" \
  'material with spaces.mcraft' '--other-name.mcraft'
# A version query answers with the plain argument, so it needs no profile at all.
expect '<--version>' --version
expect '<-V>' -V
# The Launcher's own form is passed through once, never doubled, in either accepted order.
expect "$(printf '<--launch-profile>\n<nightly-v1>\n<--profile-root>\n<%s>\n<--version>\n' "$work/launcher root")" \
  --launch-profile nightly-v1 --profile-root "$work/launcher root" --version
expect "$(printf '<--profile-root>\n<%s>\n<--launch-profile>\n<nightly-v1>\n<--version>\n' "$work/launcher root")" \
  --profile-root "$work/launcher root" --launch-profile nightly-v1 --version
# A caller-written `--` is stripped, never doubled: the app receives exactly one.
expect "$(launch "$XDG_DATA_HOME/materialcraft-nightly" 'after the delimiter.mcraft')" -- 'after the delimiter.mcraft'

echo "PASS: Arch Nightly handoff starts the flavor's profile and preserves launch arguments"
