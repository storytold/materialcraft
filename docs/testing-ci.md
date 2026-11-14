# Integration tests and CI

The engine and UI integration tests share one `it` executable per crate. This
avoids compiling and linking the same crate dependencies into a separate
executable for every feature. Feature modules keep their own helpers and tests.
When adding a file under `tests/it/`, register it with `mod name;` in that
folder's `main.rs` so Cargo runs it.

Run a feature module with a test-name filter, for example:

```sh
cargo test -p materialcraft-engine --test it graph_edit::
```

For faster optimized local verification, run the same workspace tests and doctests
with the `verify` profile:

```sh
cargo test --workspace --profile verify
```

`verify` inherits the release profile's optimization level and runtime checks, but
uses 16 codegen units and `lto = false`. Cargo retains thin LTO within each crate
without repeating cross-crate LTO for every test executable. The existing test
selection and ignore attributes still apply. Keep the target directory between
runs so Cargo can reuse unchanged build artifacts.

To compile the normal application binaries alongside the test binaries before
running them:

```sh
cargo build --workspace --bins --tests --profile verify
cargo test --workspace --profile verify
```

This profile is for local verification. Use `--release` for release validation and
performance measurements; its compiler settings remain unchanged. The regular
debug test profile and CI test selection also remain unchanged.

The language-switching test stays in its own executable because it changes the
process-global locale, which UI constructors also set.

The standalone engine `session` target and UI `snapshot` target remain
available for the commands in `AGENTS.md`. Existing ignore attributes are
unchanged. Tests in the shared executable can run concurrently across modules;
keep temporary paths unique and avoid changing process-wide state.

CI continues to use the shared gate workflow and `cargo xtask ci` on Ubuntu and
Windows, using the shared workflow's stable Rust default: formatting, layering,
Clippy, workspace tests (including doctests) and isolated per-package WebAssembly
checks. Layering runs before Clippy and tests to reject structural failures early.
The caller disables the shared workflow's optional tool installation because this
gate uses `cargo test` and does not use nextest.
