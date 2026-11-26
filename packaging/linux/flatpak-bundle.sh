#!/usr/bin/env bash
# Repackage a MaterialCraft Linux tarball as a single-file Flatpak bundle:
#
#   $DIST/materialcraft-<version>-linux-<arch>.flatpak
#
# Usage: packaging/linux/flatpak-bundle.sh [--no-test] [TARBALL]
#
# TARBALL defaults to $DIST/materialcraft-<version>-linux-<arch>.tar.gz from package.sh. The bundle
# is built for the host architecture (x86_64 or aarch64), which must match the tarball's.
# Needs flatpak, flatpak-builder and the SVG pixbuf loader (librsvg2-common) for the host's
# `appstreamcli compose`; the freedesktop runtime and SDK named in the manifest are
# installed per-user from Flathub. Unless --no-test, the bundle is then installed per-user and
# `materialcraft-cli --version` is run inside the sandbox as a smoke test.
# Manifest: packaging/linux/flatpak/ai.storyteller.MaterialCraft.bundle.yml.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=ai.storyteller.materialcraft
FLATPAK_ID="$APP_ID"
# The remote indexes both app IDs on the stable ref branch; IDs isolate installations.
FLATPAK_BRANCH=stable
if [[ "${MATERIALCRAFT_BUILD_CHANNEL:-}" == nightly-v1 || "$VERSION" == *-nightly* ]]; then
  FLATPAK_ID="$APP_ID.nightly"
fi
FLATHUB=https://dl.flathub.org/repo/flathub.flatpakrepo
# The OCI repository serves only the latest stable release, so a prerelease bundle gets no update
# source: Flatpak would otherwise replace it with that older stable build.
OCI_REPO=oci+https://getartcraft.com
if [[ "$VERSION" == *-* ]]; then OCI_REPO=""; fi

TEST=1
OCI=0
TARBALL=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-test) TEST=0; shift ;;
    --oci) OCI=1; shift ;;
    -h | --help) sed -n '2,14p' "$0"; exit 0 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) TARBALL="$1"; shift ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
BASENAME="materialcraft-$VERSION-linux-$ARCH"
TARBALL="${TARBALL:-$DIST/$BASENAME.tar.gz}"
[[ -f "$TARBALL" ]] || { echo "error: $TARBALL not found (run packaging/linux/package.sh --formats tar first)" >&2; exit 1; }
for tool in flatpak flatpak-builder; do
  command -v "$tool" >/dev/null || { echo "error: $tool not found (apt install flatpak flatpak-builder librsvg2-common)" >&2; exit 1; }
done

echo "==> MaterialCraft $VERSION Flatpak bundle for $ARCH from $(basename "$TARBALL")"

WORK="$CARGO_TARGET_DIR/flatpak-bundle"
rm -rf "$WORK"
mkdir -p "$WORK/stage"
tar -xzf "$TARBALL" -C "$WORK/stage" --strip-components=1
[[ -x "$WORK/stage/bin/materialcraft" ]] || { echo "error: $TARBALL has no bin/materialcraft" >&2; exit 1; }
if [[ "$FLATPAK_ID" != "$APP_ID" ]]; then
  # The bundled nightly starts the GUI the flavor's way. The sandbox data folder belongs to the
  # nightly app id, so the profile stays inside this channel's own instance.
  mv "$WORK/stage/bin/materialcraft" "$WORK/stage/bin/materialcraft-bin"
  cat >"$WORK/stage/bin/materialcraft" <<'WRAPPER'
#!/bin/sh
# Start the Nightly GUI with this channel's own launch profile.
set -eu
case "${1-}" in
  # The Launcher already named the profile (its version query does too), in either accepted order;
  # never pass it a second pair. A plain version or help query is answered without any profile.
  --launch-profile | --profile-root | --version | -V | --help | -h) exec /app/libexec/materialcraft "$@" ;;
esac
# A caller-written `--` is ours to strip: the app receives exactly one, from here.
if [ "${1-}" = "--" ]; then shift; fi
profile_base="${XDG_DATA_HOME:-${HOME:?HOME is required}/.var/app/ai.storyteller.materialcraft.nightly/data}"
exec /app/libexec/materialcraft \
  --launch-profile nightly-v1 --profile-root "$profile_base/materialcraft-nightly" -- "$@"
WRAPPER
  chmod 755 "$WORK/stage/bin/materialcraft"
  NIGHTLY_ICON="$ROOT/assets/app-icon/materialcraft-nightly.png"
  [[ -f "$NIGHTLY_ICON" ]] || NIGHTLY_ICON="$ROOT/assets/app-icon/hicolor/256x256/apps/$APP_ID.png"
  sed -e "s|^Icon=$APP_ID$|Icon=$FLATPAK_ID|" \
      -e "s|^Name=MaterialCraft$|Name=MaterialCraft Nightly|" \
      -e 's|^StartupWMClass=.*|StartupWMClass=ai.storyteller.materialcraft.nightly-v1|' \
      -e '/^MimeType=/d' \
      "$WORK/stage/share/applications/$APP_ID.desktop" >"$WORK/stage/share/applications/$FLATPAK_ID.desktop"
  rm -f "$WORK/stage/share/applications/$APP_ID.desktop"
  sed -e "s|<id>$APP_ID</id>|<id>$FLATPAK_ID</id>|g" \
      -e "s|<name>MaterialCraft</name>|<name>MaterialCraft Nightly</name>|g" \
      -e "s|<launchable type=\"desktop-id\">$APP_ID.desktop</launchable>|<launchable type=\"desktop-id\">$FLATPAK_ID.desktop</launchable>|g" \
      -e "s|<icon type=\"stock\">$APP_ID</icon>|<icon type=\"stock\">$FLATPAK_ID</icon>|g" \
      -e '/<mediatype>application\/x-materialcraft<\/mediatype>/d' \
      "$WORK/stage/share/metainfo/$APP_ID.metainfo.xml" >"$WORK/stage/share/metainfo/$FLATPAK_ID.metainfo.xml"
  rm -f "$WORK/stage/share/metainfo/$APP_ID.metainfo.xml"
  # The nightly registers no MIME type, so it installs no MIME database file either.
  rm -rf "$WORK/stage/share/mime"
  rm -rf "$WORK/stage/share/icons/hicolor"
  install -Dm644 "$NIGHTLY_ICON" \
    "$WORK/stage/share/icons/hicolor/256x256/apps/$FLATPAK_ID.png"
  sed -e "s|ai\.storyteller\.materialcraft|$FLATPAK_ID|g" \
      -e 's|- install -Dm755 bin/materialcraft /app/bin/materialcraft|- install -Dm755 bin/materialcraft-bin /app/libexec/materialcraft\n      - install -Dm755 bin/materialcraft /app/bin/materialcraft|' \
      -e '\|# .mcraft, plus mesh types|d' \
      -e '\|share/mime/packages|d' \
      "$HERE/flatpak/ai.storyteller.MaterialCraft.bundle.yml" >"$WORK/$FLATPAK_ID.yml"
else
  cp "$HERE/flatpak/ai.storyteller.MaterialCraft.bundle.yml" "$WORK/$FLATPAK_ID.yml"
fi

flatpak remote-add --user --if-not-exists flathub "$FLATHUB"
# --disable-rofiles-fuse: no FUSE in containers and CI runners.
flatpak-builder --user --install-deps-from=flathub --disable-rofiles-fuse --force-clean \
  --default-branch="$FLATPAK_BRANCH" --repo="$WORK/repo" --state-dir="$WORK/state" "$WORK/build" "$WORK/$FLATPAK_ID.yml"

OUT="$DIST/$BASENAME.flatpak"
# A release build (the shared release workflow sets SIGNING_FPR, SIGNING_PUBKEY and GNUPGHOME)
# signs the commit and embeds the public key, so Flatpak verifies the bundle when installing it.
GPG_KEYS=()
if [[ -n "${SIGNING_FPR:-}" ]]; then
  flatpak build-sign "$WORK/repo" "$FLATPAK_ID" "$FLATPAK_BRANCH" --gpg-sign="$SIGNING_FPR" --gpg-homedir="${GNUPGHOME:?}"
  GPG_KEYS=(--gpg-keys="${SIGNING_PUBKEY:?}")
  if [[ "${OCI_SIGNATURES_READY:-}" != 1 ]]; then OCI_REPO=""; fi
fi
# --runtime-repo lets `flatpak install` fetch the freedesktop runtime from Flathub if missing.
# --repo-url makes the installed origin update from the ArtCraft OCI repository.
flatpak build-bundle --runtime-repo="$FLATHUB" ${OCI_REPO:+--repo-url="$OCI_REPO"} "${GPG_KEYS[@]}" "$WORK/repo" "$OUT" "$FLATPAK_ID" "$FLATPAK_BRANCH"
echo "wrote $OUT"
if [[ "$OCI" = 1 ]]; then
  OUT_OCI="$DIST/$BASENAME-oci"
  rm -rf "$OUT_OCI"
  flatpak build-bundle --oci "$WORK/repo" "$OUT_OCI" "$FLATPAK_ID" "$FLATPAK_BRANCH"
  echo "wrote OCI layout to $OUT_OCI"
fi

if [[ "$TEST" = 1 ]]; then
  flatpak install --user -y --noninteractive --reinstall "$OUT"
  flatpak info --user "$FLATPAK_ID"
  flatpak run --command=materialcraft-cli "$FLATPAK_ID" --version
  # A nightly registers no MIME type, so it has no mime database directory to list.
  if [[ "$FLATPAK_ID" = "$APP_ID" ]]; then
    flatpak run --command=sh "$FLATPAK_ID" -c 'ls /app/share/applications /app/share/metainfo /app/share/mime/packages /app/share/icons/hicolor/*/apps'
  else
    flatpak run --command=sh "$FLATPAK_ID" -c 'ls /app/share/applications /app/share/metainfo /app/share/icons/hicolor/*/apps'
    # The installed command is the profile launcher, and the real GUI sits beside it.
    flatpak run --command=sh "$FLATPAK_ID" -c 'test -x /app/bin/materialcraft && test -x /app/libexec/materialcraft'
  fi
fi
echo "==> done"
ls -lh "$OUT"
