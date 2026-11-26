#!/usr/bin/env bash
# Build and package MaterialCraft for Linux (x86_64, aarch64 or cross-compiled riscv64):
#
#   $DIST/materialcraft-<version>-linux-<arch>.AppImage  any distro with glibc >= the build host's
#   $DIST/materialcraft-<version>-linux-<arch>.AppImage.zsync  delta updates (needs zsyncmake)
#   $DIST/materialcraft-<version>-linux-<arch>.deb       Debian, Ubuntu, Mint, Pop!_OS, ...
#   $DIST/materialcraft-<version>-linux-<arch>.rpm       Fedora, openSUSE, RHEL, ...
#   $DIST/materialcraft-<version>-linux-<arch>.tar.gz    plain FHS-style tree (bin/, share/)
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "appimage deb rpm tar"] [--arch-tar-dir DIRECTORY]
#
# Needs: cargo; nfpm for deb/rpm (https://nfpm.goreleaser.com); appimagetool for the AppImage
# (downloaded into $CARGO_TARGET_DIR if missing). Build on an old distro (CI: Ubuntu 22.04,
# glibc 2.35) so the binaries run on newer ones. Optional: desktop-file-validate, appstreamcli,
# zsyncmake (the zsync package) for the AppImage's .zsync.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=ai.storyteller.materialcraft

SKIP_BUILD=0
FORMATS="appimage deb rpm tar"
ARCH_TAR_DIR=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    --arch-tar-dir)
      [[ -n "${2:-}" ]] || { echo "error: --arch-tar-dir needs a directory" >&2; exit 2; }
      ARCH_TAR_DIR="$2"
      shift 2
      ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="${CROSS_ARCH:-$(uname -m)}"
case "$ARCH" in
  riscv64) DEB_ARCH=riscv64 ;;
  x86_64) DEB_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export MATERIALCRAFT_MAINTAINER="${MATERIALCRAFT_MAINTAINER:-MaterialCraft maintainers <materialcraft@storyteller.ai>}"
IS_NIGHTLY=0
if [[ "${MATERIALCRAFT_BUILD_CHANNEL:-}" == nightly-v1 || "$VERSION" == *-nightly* ]]; then
  IS_NIGHTLY=1
  export MATERIALCRAFT_BUILD_CHANNEL=nightly-v1
  # build.rs compiles the nightly channel of `MATERIALCRAFT_BUILD_CHANNEL` and embeds
  # `MATERIALCRAFT_VERSION`; there is no `_OVERRIDE_VERSION` on this app.
  export MATERIALCRAFT_VERSION="$VERSION"
fi
ARTIFACT_VERSION="$VERSION"
if [[ "$IS_NIGHTLY" = 1 && "$ARTIFACT_VERSION" != *-nightly* ]]; then
  ARTIFACT_VERSION="${ARTIFACT_VERSION}-nightly"
fi
BASENAME="materialcraft-$ARTIFACT_VERSION-linux-$ARCH"

if [[ -n "$ARCH_TAR_DIR" && "$IS_NIGHTLY" != 1 ]]; then
  echo "error: --arch-tar-dir is only for Nightly builds" >&2
  exit 2
fi

echo "==> MaterialCraft $VERSION for Linux $ARCH ($FORMATS, channel=${MATERIALCRAFT_BUILD_CHANNEL:-stable})"

if [[ "$SKIP_BUILD" = 0 ]]; then
  (cd "$ROOT" && cargo build --release --locked -p materialcraft -p materialcraft-cli ${CROSS_TARGET:+--target "$CROSS_TARGET"})
fi
BIN="$CARGO_TARGET_DIR/${CROSS_TARGET:+$CROSS_TARGET/}release"
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"

# ---- stage an FHS tree (shared by every format) -------------------------------------------------
install -Dm755 "$BIN/materialcraft" "$STAGE/usr/bin/materialcraft"
install -Dm755 "$BIN/materialcraft-cli" "$STAGE/usr/bin/materialcraft-cli"
"${CROSS_COMPILE:-}strip" "$STAGE/usr/bin/materialcraft" "$STAGE/usr/bin/materialcraft-cli" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
if [[ "$IS_NIGHTLY" = 1 ]]; then
  sed -e 's|^StartupWMClass=.*|StartupWMClass=ai.storyteller.materialcraft.nightly-v1|' \
      "$STAGE/usr/share/applications/$APP_ID.desktop" >"$STAGE/usr/share/applications/$APP_ID.desktop.tmp"
  mv "$STAGE/usr/share/applications/$APP_ID.desktop.tmp" "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$MATERIALCRAFT_BUILD_DATE/g" \
  "$HERE/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ROOT/assets/app-icon/hicolor" "$STAGE/usr/share/icons/"
mkdir -p "$STAGE/usr/share/doc/materialcraft"
copy_docs "$STAGE/usr/share/doc/materialcraft"

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net --explain "$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
fi

has() { case " $FORMATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

# ---- .tar.gz ------------------------------------------------------------------------------------
if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

# The Arch handoff keeps the same basename as the plain tarball in a separate directory.
# Flatpak, OCI and direct tar consumers still get the original binary, not this launcher.
if [[ -n "$ARCH_TAR_DIR" ]]; then
  mkdir -p "$ARCH_TAR_DIR"
  if [[ "$(cd "$ARCH_TAR_DIR" && pwd -P)" == "$(cd "$DIST" && pwd -P)" ]]; then
    echo "error: the Arch tar directory must differ from the release directory" >&2
    exit 2
  fi
  bash "$HERE/nightly-arch-tar.sh" "$STAGE/usr" "$WORK/arch-tar" "$ARCH_TAR_DIR" "$BASENAME"
fi

# ---- .deb / .rpm --------------------------------------------------------------------------------
if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm not found (https://nfpm.goreleaser.com/install/)" >&2; exit 1; }
  export VERSION
  export NFPM_ARCH="$DEB_ARCH"
  NFPM_STAGE="$STAGE"
  export NFPM_NAME="materialcraft"
  export NFPM_BIN="materialcraft"
  export NFPM_CLI="materialcraft-cli"
  if [[ "$IS_NIGHTLY" = 1 ]]; then
    NIGHTLY_DESK_ID="$APP_ID.nightly-v1"
    NIGHTLY_ICON="$ROOT/assets/app-icon/materialcraft-nightly.png"
    [[ -f "$NIGHTLY_ICON" ]] || NIGHTLY_ICON="$ROOT/assets/app-icon/hicolor/256x256/apps/$APP_ID.png"
    export NFPM_NAME="materialcraft-nightly"
    export NFPM_BIN="materialcraft-nightly"
    export NFPM_CLI="materialcraft-nightly-cli"
    NFPM_STAGE="$WORK/nfpm-root"
    rm -rf "$NFPM_STAGE"
    cp -R "$STAGE" "$NFPM_STAGE"
    install -Dm755 "$NFPM_STAGE/usr/bin/materialcraft" "$NFPM_STAGE/usr/lib/materialcraft-nightly/materialcraft"
    cat >"$NFPM_STAGE/usr/bin/$NFPM_BIN" <<'WRAPPER'
#!/bin/sh
# Start the Nightly GUI the flavor's way: this channel's own launch profile, so an installed
# nightly never reads or writes stable's settings, window state or recovery copies.
set -eu
case "${1-}" in
  # The Launcher already named the profile (its version query does too), in either accepted order;
  # never pass it a second pair. A plain version or help query is answered without any profile.
  --launch-profile | --profile-root | --version | -V | --help | -h) exec /usr/lib/materialcraft-nightly/materialcraft "$@" ;;
esac
# A caller-written `--` is ours to strip: the app receives exactly one, from here.
if [ "${1-}" = "--" ]; then shift; fi
profile_base="${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}"
exec /usr/lib/materialcraft-nightly/materialcraft \
  --launch-profile nightly-v1 --profile-root "$profile_base/materialcraft-nightly" -- "$@"
WRAPPER
    chmod 755 "$NFPM_STAGE/usr/bin/$NFPM_BIN"
    rm -f "$NFPM_STAGE/usr/bin/materialcraft"
    mv "$NFPM_STAGE/usr/bin/materialcraft-cli" "$NFPM_STAGE/usr/bin/$NFPM_CLI"
    mv "$NFPM_STAGE/usr/share/doc/materialcraft" "$NFPM_STAGE/usr/share/doc/$NFPM_NAME"
    sed -e "s#^Exec=materialcraft\([[:space:]]\|$\)#Exec=$NFPM_BIN\1#" \
        -e "s|^TryExec=materialcraft$|TryExec=$NFPM_BIN|" \
        -e "s|^Icon=$APP_ID$|Icon=$NIGHTLY_DESK_ID|" \
        -e "s|^Name=MaterialCraft$|Name=MaterialCraft Nightly|" \
        -e '/^MimeType=/d' \
        "$NFPM_STAGE/usr/share/applications/$APP_ID.desktop" >"$NFPM_STAGE/usr/share/applications/$NIGHTLY_DESK_ID.desktop"
    rm -f "$NFPM_STAGE/usr/share/applications/$APP_ID.desktop"
    sed -e "s|<id>$APP_ID</id>|<id>$NIGHTLY_DESK_ID</id>|g" \
        -e "s|<name>MaterialCraft</name>|<name>MaterialCraft Nightly</name>|g" \
        -e "s|<launchable type=\"desktop-id\">$APP_ID.desktop</launchable>|<launchable type=\"desktop-id\">$NIGHTLY_DESK_ID.desktop</launchable>|g" \
        -e "s|<icon type=\"stock\">$APP_ID</icon>|<icon type=\"stock\">$NIGHTLY_DESK_ID</icon>|g" \
        -e "s|<binary>materialcraft</binary>|<binary>$NFPM_BIN</binary>|g" \
        -e '/<mediatype>application\/x-materialcraft<\/mediatype>/d' \
        "$NFPM_STAGE/usr/share/metainfo/$APP_ID.metainfo.xml" >"$NFPM_STAGE/usr/share/metainfo/$NIGHTLY_DESK_ID.metainfo.xml"
    rm -f "$NFPM_STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
    rm -rf "$NFPM_STAGE/usr/share/mime"
    rm -rf "$NFPM_STAGE/usr/share/icons/hicolor"
    install -Dm644 "$NIGHTLY_ICON" \
      "$NFPM_STAGE/usr/share/icons/hicolor/256x256/apps/$NIGHTLY_DESK_ID.png"
  fi
  # nfpm expands env vars in fields like `version` and `arch`, but not in `contents[].src`.
  DROP_LIB_TREE=
  # shellcheck disable=SC2016
  [[ -d "$NFPM_STAGE/usr/lib" ]] || DROP_LIB_TREE='\|^  - src: \${STAGE}/usr/lib/|,+2d;'
  # shellcheck disable=SC2016
  sed "${DROP_LIB_TREE}s|\${STAGE}|$NFPM_STAGE|g;s|\${NFPM_BIN}|$NFPM_BIN|g;s|\${NFPM_CLI}|$NFPM_CLI|g" "$HERE/nfpm.yaml" >"$WORK/nfpm.yaml"
  # The riscv64 cross sysroot is Ubuntu 24.04, whose glibc floor is 2.39.
  if [[ "$ARCH" = riscv64 ]]; then sed -i "s/libc6 (>= 2.35)/libc6 (>= 2.39)/" "$WORK/nfpm.yaml"; fi
  for fmt in deb rpm; do
    if has "$fmt"; then (cd "$ROOT" && nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$DIST/$BASENAME.$fmt"); fi
  done
fi

# ---- AppImage -----------------------------------------------------------------------------------
if has appimage; then
  APPDIR="$WORK/MaterialCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  mv "$APPDIR/usr/share/doc" "$WORK/doc-unused"
  # AppRun is a script, not a symlink: it installs the .desktop entry and icons into
  # $XDG_DATA_HOME before launching, so Wayland compositors can resolve the window's
  # app_id to our icon instead of the generic Wayland logo (opt-out env in the
  # script header). Logic is covered by packaging/linux/apprun-test.sh.
  install -Dm755 "$HERE/AppRun" "$APPDIR/AppRun"
  if [[ "$IS_NIGHTLY" = 1 ]]; then
    APPIMAGE_ID="$APP_ID.nightly-v1"
    NIGHTLY_ICON="$ROOT/assets/app-icon/materialcraft-nightly.png"
    [[ -f "$NIGHTLY_ICON" ]] || NIGHTLY_ICON="$ROOT/assets/app-icon/hicolor/256x256/apps/$APP_ID.png"
    # shellcheck disable=SC2016
    awk -v old_id="$APP_ID" -v new_id="$APPIMAGE_ID" '
      $0 == "DESK_ID=" old_id { print "DESK_ID=" new_id; next }
      { print }
    ' "$APPDIR/AppRun" >"$APPDIR/AppRun.tmp"
    mv "$APPDIR/AppRun.tmp" "$APPDIR/AppRun"
    chmod 755 "$APPDIR/AppRun"
    sed -e "s|^Icon=$APP_ID$|Icon=$APPIMAGE_ID|" \
        -e "s|^Name=MaterialCraft$|Name=MaterialCraft Nightly|" \
        -e '/^MimeType=/d' \
        "$APPDIR/usr/share/applications/$APP_ID.desktop" >"$APPDIR/usr/share/applications/$APPIMAGE_ID.desktop"
    rm -f "$APPDIR/usr/share/applications/$APP_ID.desktop"
    cp "$APPDIR/usr/share/applications/$APPIMAGE_ID.desktop" "$APPDIR/$APPIMAGE_ID.desktop"
    rm -rf "$APPDIR/usr/share/icons/hicolor"
    install -Dm644 "$NIGHTLY_ICON" \
      "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APPIMAGE_ID.png"
    cp "$NIGHTLY_ICON" "$APPDIR/$APPIMAGE_ID.png"
    ln -s "$APPIMAGE_ID.png" "$APPDIR/.DirIcon"
  else
    cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
    cp "$ROOT/assets/app-icon/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
    ln -s "$APP_ID.png" "$APPDIR/.DirIcon"
  fi

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [[ -z "$TOOL" ]]; then
    TOOL="$CARGO_TARGET_DIR/appimagetool-$ARCH.AppImage"
    if [[ ! -x "$TOOL" ]]; then
      curl --proto '=https' -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
      chmod +x "$TOOL"
    fi
  fi
  # Absolute, because appimagetool runs in $DIST below (CARGO_TARGET_DIR or APPIMAGETOOL may be
  # relative, e.g. target/agent-<name>).
  OUT="$(cd "$DIST" && pwd)/$BASENAME.AppImage"
  APPDIR="$(cd "$APPDIR" && pwd)"
  TOOL="$(cd "$(dirname "$TOOL")" && pwd)/$(basename "$TOOL")"
  # A .zsync left by an earlier run would hide a missing zsyncmake and describe another file.
  rm -f "$OUT.zsync"
  # Update information: AppImageUpdate, AppImageLauncher and the like read it from the
  # file and fetch only the blocks that changed in a newer release, through the .zsync published
  # next to each AppImage on GitHub Releases. `latest` is the newest published release that is
  # not a pre-release. A fork's builds point at its own releases through GITHUB_REPOSITORY.
  REPO="${GITHUB_REPOSITORY:-storytold/materialcraft}"
  UPDATE_ARGS=()
  if [[ "$IS_NIGHTLY" = 0 ]]; then
    UPDATE_INFO="gh-releases-zsync|${REPO%%/*}|${REPO#*/}|latest|materialcraft-*-linux-$ARCH.AppImage.zsync"
    UPDATE_ARGS=(-u "$UPDATE_INFO")
  else
    # Nightly releases are GitHub pre-releases (`latest` only selects stable releases), so point
    # zsync update metadata at pre-release tags matching `-nightly` rather than stable `latest`.
    UPDATE_INFO="gh-releases-zsync|${REPO%%/*}|${REPO#*/}|latest-pre|materialcraft-*-nightly*-linux-$ARCH.AppImage.zsync"
    UPDATE_ARGS=(-u "$UPDATE_INFO")
  fi
  # Extract-and-run: works without FUSE (containers, CI). The output embeds the static runtime,
  # so users don't need libfuse2 either. With zsyncmake on the host (CI installs the zsync
  # package) appimagetool also writes the .zsync, into its working directory, hence the cd.
  (cd "$DIST" && ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "${UPDATE_ARGS[@]}" "$APPDIR" "$OUT")
  echo "wrote $OUT"
  if [[ -s "$OUT.zsync" ]]; then
    echo "wrote $OUT.zsync"
  else
    warn "zsyncmake not found, so $OUT.zsync was not written; AppImage delta updates need it"
  fi
fi

if [[ -n "${CROSS_TARGET:-}" ]]; then
  if [[ -n "${EMULATOR:-}" ]]; then "$EMULATOR" "$STAGE/usr/bin/materialcraft-cli" --version; fi
else
  "$STAGE/usr/bin/materialcraft-cli" --version
fi
echo "==> done"
ls -lh "$DIST"
