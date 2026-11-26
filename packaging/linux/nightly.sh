#!/usr/bin/env bash
# App-owned GUI-only nightly tar: no FUSE/AppImage, CLI, association or install hooks.
set -euo pipefail
ARCH=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) [[ $# -ge 2 ]] || { echo 'missing --arch value' >&2; exit 2; }; ARCH="$2"; shift 2 ;;
    -h|--help) echo 'Usage: nightly.sh --arch x86_64'; exit 0 ;;
    *) echo "unsupported nightly argument: $1" >&2; exit 2 ;;
  esac
done
[[ "$ARCH" == x86_64 && "$(uname -s)" == Linux ]] || { echo 'only native Linux x86_64 nightly packaging is supported' >&2; exit 2; }
# shellcheck source=../nightly-common.sh
. "$(dirname "${BASH_SOURCE[0]}")/../nightly-common.sh"
nightly_init linux "$ARCH" x86_64-unknown-linux-gnu
OUTPUT="$DIST/materialcraft-$VERSION-linux-x86_64-nightly.tar.gz"
[[ ! -e "$OUTPUT" && ! -L "$OUTPUT" ]] || nightly_die 'refusing to replace an existing nightly archive'
nightly_build_gui
STAGE="$NIGHTLY_WORK/stage"
mkdir -p "$STAGE"
# The standard layout: `<app>` is the launcher an unpacked tar starts, and `<app>-bin` the GUI it
# hands the channel's own profile to, so a copy outside a package is isolated too.
cp "$NIGHTLY_GUI" "$STAGE/materialcraft-bin"
chmod 755 "$STAGE/materialcraft-bin"
install -m 755 "$ROOT/packaging/linux/nightly-launch.sh" "$STAGE/materialcraft"
NIGHTLY_ICON="$ROOT/assets/app-icon/materialcraft-nightly.png"
[[ -f "$NIGHTLY_ICON" ]] || NIGHTLY_ICON="$ROOT/assets/app-icon/hicolor/256x256/apps/ai.storyteller.materialcraft.png"
cp "$NIGHTLY_ICON" "$STAGE/materialcraft-nightly.png"
nightly_copy_notices "$STAGE"
# The private mktemp cwd is the helper authority root, not a CLI path parent.
(cd "$NIGHTLY_WORK" && python3 "$ROOT/packaging/nightly-archive.py" linux stage archive.tar.gz)
nightly_publish_archive "$NIGHTLY_WORK/archive.tar.gz" "$OUTPUT"
