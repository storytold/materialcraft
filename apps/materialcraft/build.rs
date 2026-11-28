// Build provenance for every target, plus the nightly channel, the Windows icon and VERSIONINFO.
//
// Shared by `materialcraft` and `materialcraft-cli` (whose build.rs `include!`s this file).
//
// - `MATERIALCRAFT_BUILD_CHANNEL=nightly-v1` compiles the nightly flavor (`cfg(materialcraft_nightly_v1)`)
//   and requires an exact `MATERIALCRAFT_VERSION`; unset, empty or `stable` is the stable build.
// - Always sets `MATERIALCRAFT_BUILD_SHA` (full git commit) and `MATERIALCRAFT_BUILD_DATE`
//   (`YYYY-MM-DD`, UTC) for `env!`, and `MATERIALCRAFT_BUILT_VERSION` (the version this build
//   identifies itself with: the feed identity for nightly, the Cargo version otherwise).
//   They come from the same-named environment variables (CI and packaging/env.sh set them), else
//   from `git` and the clock (`SOURCE_DATE_EPOCH` wins for reproducible builds); either is an empty
//   string when unknown, so source tarballs, wasm and offline builds still compile. Without the env
//   vars the commit is read when this script runs, so a local build can show a stale commit; release
//   builds always set the variable.
// - Windows targets only: embeds assets/app-icon/materialcraft[-nightly].ico and VERSIONINFO into
//   the exe. A missing resource compiler is a warning, so a cross-compile still links, unless
//   `MATERIALCRAFT_REQUIRE_WINRES=1` (set by packaging/windows/package.ps1) makes it an error.

#[path = "src/build_channel.rs"]
mod build_channel;

use std::process::Command;

const ICON: &str = "../../assets/app-icon/materialcraft.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");
    println!("cargo:rerun-if-changed=../../assets/app-icon/materialcraft-nightly.ico");
    println!("cargo:rustc-check-cfg=cfg(materialcraft_nightly_v1)");
    for var in [
        "MATERIALCRAFT_BUILD_CHANNEL",
        "MATERIALCRAFT_VERSION",
        "MATERIALCRAFT_BUILD_SHA",
        "MATERIALCRAFT_BUILD_DATE",
        "MATERIALCRAFT_REQUIRE_WINRES",
        "SOURCE_DATE_EPOCH",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let nightly = match std::env::var("MATERIALCRAFT_BUILD_CHANNEL").as_deref() {
        Ok("nightly-v1") => {
            println!("cargo:rustc-cfg=materialcraft_nightly_v1");
            true
        }
        // An empty value (an unset CI input) means the default stable build.
        Ok("stable" | "") | Err(std::env::VarError::NotPresent) => false,
        _ => {
            eprintln!("unsupported MATERIALCRAFT_BUILD_CHANNEL (expected stable or nightly-v1)");
            std::process::exit(1);
        }
    };
    // The version this build identifies itself with. A nightly must name its exact feed identity,
    // so a nightly exe can never be packaged or shipped as a stable one.
    let built_version = if nightly {
        match std::env::var("MATERIALCRAFT_VERSION") {
            Ok(version) if build_channel::is_nightly_version(&version) => version,
            _ => {
                eprintln!("nightly-v1 requires an explicit MATERIALCRAFT_VERSION feed identity");
                std::process::exit(1);
            }
        }
    } else {
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
    };
    println!("cargo:rustc-env=MATERIALCRAFT_BUILT_VERSION={built_version}");
    println!("cargo:rustc-env=MATERIALCRAFT_BUILD_SHA={}", env_or("MATERIALCRAFT_BUILD_SHA", git_sha));
    println!("cargo:rustc-env=MATERIALCRAFT_BUILD_DATE={}", env_or("MATERIALCRAFT_BUILD_DATE", today));

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources(nightly, &built_version);
    }
}

fn env_or(var: &str, fallback: fn() -> Option<String>) -> String {
    std::env::var(var).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).or_else(fallback).unwrap_or_default()
}

fn git_sha() -> Option<String> {
    let out = Command::new("git").args(["rev-parse", "HEAD"]).output().ok().filter(|o| o.status.success())?;
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (sha.len() >= 7 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then_some(sha)
}

/// UTC date of `SOURCE_DATE_EPOCH`, or of now.
fn today() -> Option<String> {
    let secs = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(v) => v.trim().parse::<u64>().ok()?,
        Err(_) => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs(),
    };
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm, proleptic Gregorian).
    let z = i64::try_from(secs / 86_400).ok()? + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn windows_resources(nightly: bool, built_version: &str) {
    let name = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "materialcraft".into());
    let cli = name.ends_with("-cli");
    let description = if cli {
        "MaterialCraft command-line tool"
    } else if nightly {
        "MaterialCraft Nightly procedural material designer"
    } else {
        "MaterialCraft procedural material designer"
    };
    let mut res = winresource::WindowsResource::new();
    res.set_icon(build_channel::windows_icon_path(nightly))
        .set("ProductName", if nightly && !cli { "MaterialCraft Nightly" } else { "MaterialCraft" })
        .set("FileDescription", description)
        .set("CompanyName", "Learning Machines LLC")
        .set("LegalCopyright", "Copyright (c) the MaterialCraft contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", &format!("{name}.exe"))
        .set("InternalName", &name);
    if nightly && !cli {
        // Launcher verifies the PE ProductVersion against the exact feed identity, not FileVersion.
        res.set("ProductVersion", built_version);
    }
    if let Err(e) = res.compile() {
        if std::env::var_os("MATERIALCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources into {name}.exe failed: {e}");
        } else {
            println!("cargo:warning={name}.exe built without icon/version resources: {e}");
        }
    }
}
