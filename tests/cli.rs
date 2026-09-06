use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netboard"))
        .args(args)
        .output()
        .expect("run netboard CLI")
}

#[test]
fn help_exits_successfully_without_opening_gui() {
    for args in [vec!["--help"], vec!["--cli", "--help"], vec!["--cli", "-h"]] {
        let output = run(&args);
        assert!(output.status.success());
        let help = String::from_utf8(output.stderr).unwrap();
        assert!(help.contains("netboard --cli [-a] [-c] [-n] [process|pid]"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn invalid_arguments_exit_two_before_collecting_sockets() {
    for (args, expected) in [
        (vec!["--cli", "--bogus"], "unknown flag: --bogus"),
        (
            vec!["--cli", "first", "second"],
            "only one process/PID filter allowed",
        ),
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8(output.stderr).unwrap().contains(expected));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn numeric_csv_for_impossible_pid_contains_only_header() {
    // Darwin PIDs are signed 32-bit; this filter cannot expose real process data.
    let output = run(&["--cli", "-n", "-a", "-c", "4294967295"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Process,PID,Proto,Dir,Local,Remote,State,Path\n"
    );
}

#[test]
fn numeric_text_for_impossible_pid_is_empty() {
    let output = run(&["--cli", "-n", "4294967295"]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
