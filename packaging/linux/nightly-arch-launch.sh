#!/bin/sh
# The Arch Nightly launcher: the GUI starts with this channel's own launch profile, so the AUR
# package never reads or writes stable's settings, window state or recovery copies.
set -eu

real_bin="$(dirname "$0")/.materialcraft-nightly/materialcraft"
case "${1-}" in
  # The Launcher already named the profile (its version query does too), in either accepted order;
  # never pass it a second pair. A plain version or help query is answered without any profile.
  --launch-profile | --profile-root | --version | -V | --help | -h) exec "$real_bin" "$@" ;;
esac
# A caller-written `--` is ours to strip: the app receives exactly one, from here.
if [ "${1-}" = "--" ]; then shift; fi
profile_base="${XDG_DATA_HOME:-${HOME:?HOME is required}/.local/share}"
exec "$real_bin" --launch-profile nightly-v1 --profile-root "$profile_base/materialcraft-nightly" -- "$@"
