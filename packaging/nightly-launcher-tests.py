#!/usr/bin/env python3
"""Every Linux Nightly launcher starts the GUI the flavor's way; no build, no window.

The deb/rpm wrapper and the Flatpak wrapper are generated inside package.sh and
flatpak-bundle.sh; the AppImage ships `AppRun` with its desktop id rewritten; the Arch handoff ships
its own launcher file. All of them must hand the binary the one protocol the flavor accepts --
`--launch-profile nightly-v1 --profile-root <dir> -- files...` -- and pass a version query through
untouched, because a nightly that starts without its profile is refused. The plain tar's launcher is
not one of ours: it is craft-common-actions' template rendered for this app, so it is checked against
the bytes it must have and the exact tokens that template passes.
"""
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
LINUX = HERE / "linux"
PROFILE = "materialcraft-nightly"
# The plain tar's launcher is craft-common-actions' template rendered for this app; these are its bytes.
TAR_PROFILE = "ai.storyteller.materialcraft.nightly"
TAR_SHA256 = "ab578db5556368f1cbfdf2f098c46c3edbbd1a23f072b938a46c021e31bbcad2"
STABLE_DESK_ID = "ai.storyteller.materialcraft"
NIGHTLY_DESK_ID = "ai.storyteller.materialcraft.nightly-v1"
RECORDER = '#!/bin/sh\nfor arg in "$@"; do printf "<%s>\\n" "$arg"; done\n'


def heredoc(script, marker, needle):
    """The one heredoc body in `script` delimited by `marker` that contains `needle`."""
    text = Path(script).read_text(encoding="utf-8")
    bodies = []
    start = 0
    while (opening := text.find("<<'%s'" % marker, start)) != -1:
        body_start = text.index("\n", opening) + 1
        body_end = text.index("\n%s\n" % marker, body_start)
        bodies.append(text[body_start:body_end] + "\n")
        start = body_end + 1
    matched = [body for body in bodies if needle in body]
    if len(matched) != 1:
        raise AssertionError(f"expected one {marker} heredoc with {needle!r} in {script}, found {len(matched)}")
    return matched[0]


class Launcher(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name).resolve()
        self.home = self.root / "home with space"
        self.home.mkdir()
        self.real = self.root / "materialcraft-real"
        self.real.write_text(RECORDER, encoding="utf-8")
        self.real.chmod(0o755)

    def launcher(self, name, body):
        path = self.root / name
        path.write_text(body, encoding="utf-8")
        path.chmod(0o755)
        return path

    def generated(self, script, marker, needle, replacement):
        body = heredoc(script, marker, needle)
        return self.launcher("materialcraft-nightly", body.replace(needle, str(replacement)))

    def recording(self, launcher, *args, env=None):
        environment = {"PATH": "/usr/bin:/bin", "HOME": str(self.home), **(env or {})}
        result = subprocess.run([str(launcher), *args], capture_output=True, text=True, env=environment, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        return [line[1:-1] for line in result.stdout.splitlines() if line.startswith("<")]

    def profile_base(self, env=None, fallback=(".local", "share")):
        return Path((env or {}).get("XDG_DATA_HOME") or self.home.joinpath(*fallback)) / PROFILE

    def assert_starts_the_profile(self, launcher, env=None, fallback=(".local", "share")):
        root = str(self.profile_base(env, fallback))
        files = ["material with spaces.mcraft", "with -- dashes.mcraft"]
        self.assertEqual(
            self.recording(launcher, *files, env=env),
            ["--launch-profile", "nightly-v1", "--profile-root", root, "--", *files],
            launcher,
        )
        self.assertEqual(self.recording(launcher, env=env), ["--launch-profile", "nightly-v1", "--profile-root", root, "--"])
        for flag in ("--version", "-V", "--help", "-h"):
            self.assertEqual(self.recording(launcher, flag, env=env), [flag], launcher)
        query = ["--launch-profile", "nightly-v1", "--profile-root", str(self.root / "launcher root"), "--version"]
        self.assertEqual(self.recording(launcher, *query, env=env), query, "a Launcher query is passed through once")
        reversed_order = query[2:] + query[:2]
        self.assertEqual(self.recording(launcher, *reversed_order, env=env), reversed_order, "the reversed profile order too")
        # A caller-written `--` is stripped, never doubled: the app sees exactly one.
        self.assertEqual(
            self.recording(launcher, "--", "after the delimiter.mcraft", env=env),
            ["--launch-profile", "nightly-v1", "--profile-root", root, "--", "after the delimiter.mcraft"],
            launcher,
        )

    def test_the_plain_tar_launcher_is_the_shared_template(self):
        # The plain tar's `materialcraft` is the shared template, so it names the GUI beside it and
        # the package identity's profile folder, and it always passes exactly one `--`.
        (self.root / "materialcraft-bin").write_text(RECORDER, encoding="utf-8")
        (self.root / "materialcraft-bin").chmod(0o755)
        source = (LINUX / "nightly-launch.sh").read_bytes()
        self.assertEqual(hashlib.sha256(source).hexdigest(), TAR_SHA256,
                         "the tar launcher must stay byte for byte scripts/nightly-launcher.sh.in rendered for materialcraft")
        installed = self.launcher("materialcraft", source.decode("utf-8"))
        for env in (None, {"XDG_DATA_HOME": str(self.root / "xdg with space")}):
            with self.subTest(env=env):
                root = str(Path((env or {}).get("XDG_DATA_HOME") or self.home / ".local/share") / TAR_PROFILE)
                tokens = ["--launch-profile", "nightly-v1", "--profile-root", root]
                self.assertEqual(
                    self.recording(installed, "graph.mcraft", "--dashes.mcraft", env=env),
                    [*tokens, "--", "graph.mcraft", "--dashes.mcraft"],
                )
                self.assertEqual(self.recording(installed, env=env), [*tokens, "--"])
                # A version query is asked in the Launcher's own form, so the GUI answers first.
                for flag in ("--version", "-V"):
                    self.assertEqual(self.recording(installed, flag, env=env), [*tokens, "--version"])
                # The ArtCraft Launcher names its own profile and per-install root: passed unchanged.
                launcher_argv = ["--launch-profile", "nightly-v1", "--profile-root", str(self.root / "launcher root"), "--", "graph.mcraft"]
                self.assertEqual(self.recording(installed, *launcher_argv, env=env), launcher_argv)

    def test_the_arch_launcher_starts_the_profile(self):
        (self.root / ".materialcraft-nightly").mkdir()
        shutil.copy(self.real, self.root / ".materialcraft-nightly" / "materialcraft")
        installed = self.launcher("materialcraft-nightly", (LINUX / "nightly-arch-launch.sh").read_text(encoding="utf-8"))
        self.assert_starts_the_profile(installed)
        self.assert_starts_the_profile(installed, {"XDG_DATA_HOME": str(self.root / "xdg with space")})

    def test_the_deb_and_rpm_wrapper_starts_the_profile(self):
        generated = self.generated(LINUX / "package.sh", "WRAPPER", "/usr/lib/materialcraft-nightly/materialcraft", self.real)
        self.assert_starts_the_profile(generated)
        self.assert_starts_the_profile(generated, {"XDG_DATA_HOME": str(self.root / "xdg with space")})

    def test_the_appimage_apprun_starts_the_profile(self):
        # package.sh marks the Nightly AppImage by rewriting this one line; the script derives the
        # channel, the profile folder and the window identity from it.
        source = LINUX / "package.sh"
        self.assertIn('$0 == "DESK_ID=" old_id', source.read_text(encoding="utf-8"), "package.sh rewrites DESK_ID")
        apprun = (LINUX / "AppRun").read_text(encoding="utf-8")
        self.assertIn(f"DESK_ID={STABLE_DESK_ID}\n", apprun)
        nightly = apprun.replace(f"DESK_ID={STABLE_DESK_ID}\n", f"DESK_ID={NIGHTLY_DESK_ID}\n")
        appdir = self.root / "MaterialCraft.AppDir"
        (appdir / "usr/bin").mkdir(parents=True)
        shutil.copy(self.real, appdir / "usr/bin" / "materialcraft")
        (appdir / "AppRun").write_text(nightly, encoding="utf-8")
        (appdir / "AppRun").chmod(0o755)
        self.assert_starts_the_profile(appdir / "AppRun")
        self.assert_starts_the_profile(appdir / "AppRun", {"XDG_DATA_HOME": str(self.root / "xdg with space")})

    def test_the_stable_apprun_still_starts_plainly(self):
        appdir = self.root / "MaterialCraft.AppDir"
        (appdir / "usr/bin").mkdir(parents=True)
        shutil.copy(self.real, appdir / "usr/bin" / "materialcraft")
        shutil.copy(LINUX / "AppRun", appdir / "AppRun")
        self.assertEqual(self.recording(appdir / "AppRun", "graph.mcraft"), ["graph.mcraft"])
        self.assertEqual(self.recording(appdir / "AppRun", "--version"), ["--version"])

    def test_the_flatpak_wrapper_starts_the_profile(self):
        generated = self.generated(LINUX / "flatpak-bundle.sh", "WRAPPER", "/app/libexec/materialcraft", self.real)
        # Inside the bundle XDG_DATA_HOME is the nightly app's own sandbox folder.
        sandbox = self.home / ".var/app/ai.storyteller.materialcraft.nightly/data"
        self.assert_starts_the_profile(generated, {"XDG_DATA_HOME": str(sandbox)})
        self.assert_starts_the_profile(generated, fallback=(".var", "app", "ai.storyteller.materialcraft.nightly", "data"))


if __name__ == "__main__":
    unittest.main()
