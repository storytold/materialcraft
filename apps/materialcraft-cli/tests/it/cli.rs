//! RFC-0018 rule 2.7: help, versions, usage errors with exit 2 on stderr, and nothing written
//! before the arguments are checked.

use std::ffi::OsString;

use crate::{argv, invoke, package, workdir};

const SUBCOMMANDS: [&str; 12] = ["info", "publish", "commands", "nodes", "run", "batch", "new", "cook", "export", "render", "mcp", "serve"];

/// The shortest valid command line of each subcommand.
fn minimal(sub: &str) -> Vec<&str> {
    match sub {
        "info" => vec!["info", "sample"],
        "publish" => vec!["publish", "sample", "o.mcmat", "--graph", "0"],
        "run" => vec!["run", "sample", "graph.cook"],
        "batch" => vec!["batch", "s.json"],
        "new" => vec!["new", "pbr", "o.sbs"],
        "cook" => vec!["cook", "sample"],
        "export" => vec!["export", "sample", "dir"],
        "render" => vec!["render", "sample", "o.png"],
        other => vec![other],
    }
}

fn usage_error(args: &[&str]) -> String {
    let got = invoke(args.iter().copied());
    assert_eq!(got.code, 2, "{args:?}: {}", got.err);
    assert_eq!(got.out, "", "{args:?} wrote to stdout");
    assert!(got.err.starts_with("error: "), "{args:?}: {}", got.err);
    got.err
}

#[test]
fn help_and_the_version_go_to_stdout_with_exit_0() {
    for args in [&["--help"][..], &["-h"], &["help"], &["--version"], &["-V"]] {
        let got = invoke(args.iter().copied());
        assert_eq!((got.code, got.err.as_str()), (0, ""), "{args:?}");
        assert!(got.out.contains("materialcraft-cli"), "{args:?}: {}", got.out);
    }
    let help = invoke(["--help"]).out;
    assert!(help.contains("Exit status:") && help.contains("128 + the signal number"), "{help}");
    for sub in SUBCOMMANDS {
        for flag in ["--help", "-h"] {
            let got = invoke([sub, flag]);
            assert_eq!((got.code, got.err.as_str()), (0, ""), "{sub} {flag}");
            assert!(got.out.contains(&format!("Usage: materialcraft-cli {sub}")), "{sub}: {}", got.out);
        }
        assert_eq!(invoke(["help", sub]).out, invoke([sub, "--help"]).out, "{sub}");
    }
}

#[test]
fn every_subcommand_takes_the_version_and_there_is_no_version_subcommand() {
    let top = invoke(["--version"]).out;
    let want = top.strip_prefix("materialcraft-cli ").unwrap().trim_end().to_string();
    assert!(want.starts_with(materialcraft_engine::VERSION), "{want}");
    for sub in SUBCOMMANDS {
        for flag in ["--version", "-V"] {
            let got = invoke([sub, flag]);
            assert_eq!((got.code, got.out.as_str(), got.err.as_str()), (0, format!("materialcraft-cli-{sub} {want}\n").as_str(), ""), "{sub} {flag}");
        }
    }
    usage_error(&["version"]);
}

#[test]
fn no_arguments_is_a_usage_error_that_prints_the_help_on_stderr() {
    let got = invoke(Vec::<OsString>::new());
    assert_eq!((got.code, got.out.as_str()), (2, ""));
    assert!(got.err.contains("Usage: materialcraft-cli <COMMAND>"), "{}", got.err);
}

#[test]
fn an_unknown_flag_or_subcommand_is_refused_by_every_subcommand() {
    for sub in SUBCOMMANDS {
        let mut args = minimal(sub);
        args.push("--fromat");
        let message = usage_error(&args);
        assert!(message.contains("--fromat"), "{sub}: {message}");
    }
    usage_error(&["rendre"]);
    usage_error(&["--fromat"]);
}

#[test]
fn a_flag_given_twice_is_refused_unless_it_is_repeatable() {
    usage_error(&["new", "pbr", "o.sbs", "--size", "8", "--size", "9"]);
    usage_error(&["render", "sample", "o.png", "--hq", "--hq"]);
    usage_error(&["mcp", "--read-root", "a", "--automation-read-root", "b"]);
    // `--set` and `--input` repeat: clap takes both, and the graph then refuses the unknown inputs (exit 1).
    let got = invoke(["cook", "sample", "--set", "a=1", "--set", "b=2"]);
    assert_eq!(got.code, 1, "{}", got.err);
    assert!(got.err.starts_with("error: --set a: "), "{}", got.err);
}

#[test]
fn bad_values_conflicts_and_missing_arguments_are_refused_before_any_file_is_written() {
    let dir = workdir("refused");
    let out = dir.join("never");
    let o = out.to_str().unwrap();
    for (args, expect) in [
        (vec!["new", "pbr", o, "--size", "0"], "--size"),
        (vec!["new", "pbr", o, "--size", "big"], "--size"),
        (vec!["export", "sample", o, "--depth", "12"], "8, 16 or 32"),
        (vec!["export", "sample", o, "--format", "gif"], "--format"),
        (vec!["export", "sample", o, "--profile", "everything"], "--profile"),
        (vec!["export", "sample", o, "--size", "0x5"], "pixels or WIDTHxHEIGHT"),
        (vec!["export", "sample", o, "--normal-format", "opengl"], "--profile"),
        (vec!["export", "sample", o, "--profile", "maps", "--format", "png"], "cannot be used with"),
        (vec!["export", "sample", o, "--profile", "materialx", "--displacement-scale", "NaN"], "displacement-scale"),
        (vec!["export", "sample", o, "--displacement-scale", "2"], "--profile"),
        (vec!["export", "sample", o, "--out", o], "cannot be used with"),
        (vec!["export", "sample"], "required"),
        (vec!["export", "sample", o, "--graph", "1", "--graph-path", "1"], "cannot be used with"),
        (vec!["cook", "sample", "--set", "novalue"], "<identifier>=<value>"),
        (vec!["cook", "sample", "--set", "=3"], "<identifier>=<value>"),
        (vec!["cook", "sample", "--input", "x"], "<identifier>=<value>"),
        (vec!["publish", "sample", o], "required"),
        (vec!["render", "sample", o, "--size", "10"], "<W>x<H>"),
        (vec!["render", "sample", o, "--view", "4d"], "--view"),
        (vec!["render", "sample", o, "--scene", "nope"], "is not a scene"),
        (vec!["render", "sample", o, "--yaw", "NaN"], "--yaw"),
        (vec!["render", "sample", o, "--displacement", "0.1"], "--hq"),
        (vec!["run", "sample", "graph.cook", "{not json"], "not JSON"),
        (vec!["mcp", "--token", "abcdefghijklmnop"], "--bridge"),
        (vec!["mcp", "--bridge", "9878", "--read-root", o], "headless server"),
        (vec!["mcp", "--bridge", "99999"], "--bridge"),
        (vec!["serve", "--port", "http"], "--port"),
    ] {
        let message = usage_error(&args);
        assert!(message.contains(expect), "{args:?}: {message}");
    }
    assert!(!out.exists());
    assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_value_starting_with_a_dash_is_written_after_a_double_dash_and_negative_numbers_are_taken_where_they_mean_something() {
    let got = invoke(["info", "--", "-odd.sbs"]);
    assert_eq!(got.code, 1);
    assert!(got.err.contains("-odd.sbs"), "{}", got.err);
    // A value flag never takes the next flag as its value.
    usage_error(&["render", "sample", "o.png", "--scene", "--hq"]);
    // Angles are negative where they mean something.
    let dir = workdir("negative");
    let png = dir.join("n.png");
    let got = invoke(argv(&[&"render", &"sample", &png, &"--size", &"16x16", &"--yaw", &"-30", &"--pitch", &"-10"]));
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(png.exists());
    let report: serde_json::Value = serde_json::from_str(&got.out).unwrap();
    assert_eq!((report["width"].as_u64(), report["height"].as_u64()), (Some(16), Some(16)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn commands_nodes_info_and_cook_print_json() {
    for args in [vec!["commands"], vec!["commands", "--filter", "graph"], vec!["nodes", "--category", "noise"], vec!["info", "sample"], vec!["cook", "sample"]]
    {
        let got = invoke(&args);
        assert_eq!((got.code, got.err.as_str()), (0, ""), "{args:?}");
        assert!(serde_json::from_str::<serde_json::Value>(&got.out).is_ok(), "{args:?}: {}", got.out);
    }
}

#[test]
fn new_run_publish_and_export_through_files() {
    let dir = workdir("files");
    let source = package(&dir, "source.sbs");
    let s = source.to_str().unwrap();
    let got = invoke(["run", s, "graph.cook"]);
    assert_eq!((got.code, got.err.as_str()), (0, ""));
    let mcmat = dir.join("oak.mcmat");
    let got = invoke(["publish", s, mcmat.to_str().unwrap(), "--graph", "Oak"]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(mcmat.exists());
    let folder = dir.join("maps");
    let got = invoke(["export", mcmat.to_str().unwrap(), folder.to_str().unwrap(), "--profile", "maps", "--size", "16"]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(folder.is_dir());
    let again = dir.join("maps2");
    let got = invoke(["export", mcmat.to_str().unwrap(), "--out", again.to_str().unwrap(), "--profile", "maps", "--size", "16"]);
    assert_eq!(got.code, 0, "{}", got.err);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_output_that_would_replace_an_input_is_refused_without_in_place() {
    let dir = workdir("replace");
    let source = package(&dir, "a.sbs");
    let a = source.to_str().unwrap();
    let before = std::fs::read(&source).unwrap();
    let again = dir.join(".").join("a.sbs");
    let again = again.to_str().unwrap();
    for args in
        [vec!["run", a, "graph.cook", "-o", a], vec!["run", a, "graph.cook", "-o", again], vec!["publish", a, a, "--graph", "0"], vec!["render", a, again]]
    {
        let message = usage_error(&args);
        assert!(message.contains("would replace"), "{args:?}: {message}");
    }
    assert_eq!(std::fs::read(&source).unwrap(), before, "a refused command changes nothing");
    // `--in-place` lets only `run -o` replace; it needs an -o that is the input.
    let got = invoke(["run", a, "graph.cook", "-o", a, "--in-place"]);
    assert_eq!(got.code, 0, "{}", got.err);
    let elsewhere = dir.join("b.sbs");
    assert!(usage_error(&["run", a, "graph.cook", "-o", elsewhere.to_str().unwrap(), "--in-place"]).contains("--in-place"));
    usage_error(&["run", a, "graph.cook", "--in-place"]);
    assert!(!elsewhere.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn batch_runs_a_script_and_a_failing_step_fails_the_command() {
    let dir = workdir("batch");
    let good = dir.join("good.json");
    std::fs::write(&good, r#"[{"id": "graph.cook", "params": {}}]"#).unwrap();
    let got = invoke(argv(&[&"batch", &good]));
    assert_eq!((got.code, got.err.as_str()), (0, ""), "{}", got.out);
    let bad = dir.join("bad.json");
    std::fs::write(&bad, r#"[{"id": "no.such.command"}]"#).unwrap();
    let got = invoke(argv(&[&"batch", &bad]));
    assert_eq!(got.code, 1);
    assert!(got.err.contains("the batch stopped at an error"), "{}", got.err);
    let junk = dir.join("junk.json");
    std::fs::write(&junk, "[").unwrap();
    let got = invoke(argv(&[&"batch", &junk]));
    assert_eq!(got.code, 1);
    assert!(got.err.starts_with("error: script: "), "{}", got.err);
    let got = invoke(argv(&[&"batch", &dir.join("missing.json")]));
    assert_eq!(got.code, 1);
    assert!(got.err.contains("missing.json"), "{}", got.err);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_command_exits_1_with_the_message_on_stderr() {
    let got = invoke(["run", "sample", "no.such.command"]);
    assert_eq!((got.code, got.out.as_str()), (1, ""));
    assert!(got.err.starts_with("error: "), "{}", got.err);
    let got = invoke(["info", "missing.sbs"]);
    assert_eq!(got.code, 1);
    assert!(got.err.starts_with("error: ") && got.err.contains("missing.sbs"), "{}", got.err);
}

#[test]
fn the_launchers_nightly_protocol_is_refused_before_any_subcommand() {
    for args in [vec!["--launch-profile", "nightly-v1", "--version"], vec!["--profile-root=/tmp/p", "--version"], vec!["info", "sample", "--launch-profile=x"]]
    {
        let message = usage_error(&args);
        assert!(message.contains("Nightly GUI's protocol"), "{args:?}: {message}");
    }
    assert_eq!(invoke(["run", "sample", "graph.cook", "{}", "-o", "x.sbs", "--in-place"]).code, 2);
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_valid_unicode_reaches_the_command() {
    use std::os::unix::ffi::OsStringExt;
    let dir = workdir("non-unicode");
    // `render` writes through `Path`: it works.
    let png = dir.join(OsString::from_vec(b"caf\xe9.png".to_vec()));
    let got = invoke(argv(&[&"render", &"sample", &png, &"--size", &"8x8"]));
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(png.exists());
    // The engine's commands take their paths as JSON text: the command says so and names the file.
    let input = dir.join(OsString::from_vec(b"in-\xff.sbs".to_vec()));
    let got = invoke(argv(&[&"info", &input]));
    assert_eq!(got.code, 1);
    assert!(got.err.contains("the path is not valid Unicode") && got.err.contains("in-"), "{}", got.err);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_closed_stdout_is_not_an_error() {
    struct Closed;
    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut err = Vec::new();
    assert_eq!(materialcraft_cli::run(["commands"], &mut Closed, &mut err), 0);
    assert_eq!(materialcraft_cli::run(["--help"], &mut Closed, &mut err), 0);
    assert!(err.is_empty());
}
