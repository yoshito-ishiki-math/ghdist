use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ghsp-rust-exact-search"))
        .args(args)
        .output()
        .unwrap()
}

fn assert_error(output: Output, expected: &str) {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "{stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("EXACT_RESULT"));
}

#[test]
fn documented_two_point_examples_return_exact_distances() {
    for suffix in [
        vec![],
        vec!["--hyperspace", "--engine", "auto", "--compare-base"],
    ] {
        let mut args = vec!["--left-edges", "2", "--right-edges", "6"];
        args.extend(suffix);
        let output = run(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("distortion=4 d_GH=2 "));
    }
}

#[test]
fn invalid_inputs_and_zero_timeout_fail_clearly() {
    for (args, expected) in [
        (
            vec!["--left-edges", "1,1,3", "--right-edges", "2"],
            "triangle inequality",
        ),
        (
            vec!["--left-edges", "0", "--right-edges", "2"],
            "zero length",
        ),
        (vec!["--left-edges", "2"], "provide both"),
        (
            vec!["--known-pair", "--z3-timeout-seconds", "0"],
            "must be positive",
        ),
        (
            vec!["--known-pair", "--max-relation-vertices", "1"],
            "safety limit",
        ),
    ] {
        assert_error(run(&args), expected);
    }
}

#[test]
fn interrupted_search_has_no_exact_result() {
    assert_error(
        run(&[
            "--known-pair",
            "--hyperspace",
            "--engine",
            "lazy-generic",
            "--max-search-nodes",
            "0",
        ]),
        "search-node limit",
    );
}

#[test]
fn missing_optional_solver_is_an_error_in_explicit_z3_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_ghsp-rust-exact-search"))
        .args([
            "--left-family",
            "rank_generic",
            "--right-family",
            "dyadic_line",
            "--order",
            "5",
            "--hyperspace",
            "--engine",
            "z3",
            "--compare-base",
        ])
        .env(
            "PATH",
            std::env::temp_dir().join(format!("ghsp-no-z3-{}", std::process::id())),
        )
        .output()
        .unwrap();
    assert_error(output, "failed to start optional Z3 backend");
}
