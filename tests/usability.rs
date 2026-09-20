use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ghdist-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn json(&self, name: &str, value: Value) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        path
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_ghdist")
}
fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
}
fn run(args: &[&str]) -> Output {
    Command::new(binary()).args(args).output().unwrap()
}
fn decoded(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {} / {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn rational_example_singleton_validation_and_human_output() {
    let x = example("X.json");
    let y = example("Y.json");
    let p = example("point.json");
    for command in ["distance", "hyperspace"] {
        let output = run(&[command, x.to_str().unwrap(), y.to_str().unwrap(), "--json"]);
        assert!(output.status.success());
        assert_eq!(decoded(&output)["d_gh"], "1/12");
        let output = run(&[command, p.to_str().unwrap(), x.to_str().unwrap(), "--json"]);
        assert_eq!(decoded(&output)["d_gh"], "1/6");
    }
    let output = run(&["validate", p.to_str().unwrap(), "--json"]);
    assert!(output.status.success());
    assert_eq!(decoded(&output)["points"], 1);
    let output = run(&["distance", x.to_str().unwrap(), y.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("d_GH = 1/12 (exact)"));
    assert!(!text.contains("search_nodes"));
    let output = run(&[
        "distance",
        x.to_str().unwrap(),
        y.to_str().unwrap(),
        "--verbose",
    ]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("search_nodes"));
}
#[test]
fn certificate_roundtrip_and_existing_outputs_are_preserved() {
    let temp = Temp::new();
    let cert = temp.path("witness.json");
    let result = temp.path("result.json");
    let output = run(&[
        "hyperspace",
        example("ultrametric-X.json").to_str().unwrap(),
        example("ultrametric-Y.json").to_str().unwrap(),
        "--engine",
        "ultrametric",
        "--certificate",
        cert.to_str().unwrap(),
        "--output",
        result.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&std::fs::read(&result).unwrap()).unwrap();
    assert_eq!(report["d_gh"], "0");
    let output = run(&["verify", cert.to_str().unwrap(), "--json"]);
    assert!(output.status.success());
    assert_eq!(decoded(&output)["optimality_verified"], false);
    let stopped = run(&[
        "verify",
        cert.to_str().unwrap(),
        "--time-limit",
        "0",
        "--json",
    ]);
    assert_eq!(stopped.status.code(), Some(2));
    assert_eq!(decoded(&stopped)["status"], "incomplete");
    let original = std::fs::read(&result).unwrap();
    let output = run(&[
        "distance",
        example("X.json").to_str().unwrap(),
        example("Y.json").to_str().unwrap(),
        "--output",
        result.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(decoded(&output)["status"], "error");
    assert_eq!(std::fs::read(&result).unwrap(), original);
}
#[test]
fn timeout_returns_bounds_and_progress_does_not_pollute_json() {
    let output = run(&[
        "hyperspace",
        example("known-X.json").to_str().unwrap(),
        example("known-Y.json").to_str().unwrap(),
        "--time-limit",
        "0",
        "--progress",
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let value = decoded(&output);
    assert_eq!(value["status"], "bounded");
    assert!(value["d_gh"].is_null());
    assert!(value["lower_bound"].is_string() && value["upper_bound"].is_string());
    assert!(String::from_utf8_lossy(&output.stderr).contains("d_GH"));
}
#[test]
fn batches_resolve_relative_paths_and_flush_successes_and_errors() {
    let temp = Temp::new();
    temp.json("a.json", json!({"distances":[[0,2],[2,0]]}));
    temp.json("b.json", json!({"distances":[[0,6],[6,0]]}));
    let manifest=temp.json("cases.json",json!({"pairs":[["a.json","b.json"],["a.json","missing.json"],["b.json","a.json"]],"mode":"distance"}));
    let output_file = temp.path("results.jsonl");
    let output = run(&[
        "batch",
        manifest.to_str().unwrap(),
        "--output",
        output_file.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let content = std::fs::read_to_string(output_file).unwrap();
    let rows: Vec<Value> = content
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["d_gh"], "2");
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[2]["d_gh"], "2");
    assert_eq!(content, String::from_utf8_lossy(&output.stdout));
}
#[test]
fn batch_certificate_files_can_be_reverified() {
    let temp = Temp::new();
    let certs = temp.path("certificates");
    let output = run(&[
        "batch",
        example("hyperspace-batch.json").to_str().unwrap(),
        "--certificate-dir",
        certs.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    for index in 1..=2 {
        let path = certs.join(format!("pair-{index:06}.json"));
        let output = run(&["verify", path.to_str().unwrap(), "--json"]);
        assert!(output.status.success());
        assert_eq!(decoded(&output)["status"], "verified-upper-bound");
    }
}
#[test]
fn inappropriate_options_and_bad_inputs_report_json_errors() {
    for args in [
        vec!["distance", "--json"],
        vec![
            "validate",
            example("X.json").to_str().unwrap(),
            "--engine",
            "z3",
            "--json",
        ],
        vec!["verify", example("X.json").to_str().unwrap(), "--json"],
        vec![
            "distance",
            example("X.json").to_str().unwrap(),
            example("Y.json").to_str().unwrap(),
            "--time-limit",
            "NaN",
            "--json",
        ],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(decoded(&output)["status"], "error");
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    fn fake_solver(temp: &Temp) -> PathBuf {
        let marker = temp.path("solver.pid");
        let solver = temp.path("z3");
        let escaped = marker.to_string_lossy().replace('\'', "'\\''");
        std::fs::write(
            &solver,
            format!("#!/bin/sh\necho $$ > '{escaped}'\nexec /bin/sleep 20\n"),
        )
        .unwrap();
        std::fs::set_permissions(&solver, std::fs::Permissions::from_mode(0o755)).unwrap();
        marker
    }
    fn wait_for_marker(marker: &Path, child: &mut std::process::Child) {
        let started = Instant::now();
        while !marker.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "calculation finished before launching solver"
            );
            if started.elapsed() > Duration::from_secs(5) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("solver did not start");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn assert_reaped(marker: &Path) {
        let pid = std::fs::read_to_string(marker)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "solver process should have been reaped"
        );
    }
    #[test]
    fn global_deadline_kills_and_reaps_a_blocked_solver() {
        let temp = Temp::new();
        let marker = fake_solver(&temp);
        let started = Instant::now();
        let mut child = Command::new(binary())
            .args([
                "hyperspace",
                example("known-X.json").to_str().unwrap(),
                example("known-Y.json").to_str().unwrap(),
                "--engine",
                "z3",
                "--time-limit",
                "2s",
                "--json",
            ])
            .env("PATH", &temp.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Observe the blocked solver before checking that the overall deadline
        // terminates it. Leave headroom for process startup on loaded runners.
        wait_for_marker(&marker, &mut child);
        let output = child.wait_with_output().unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(decoded(&output)["status"], "bounded");
        assert_reaped(&marker);
    }
    #[test]
    fn ctrl_c_preserves_batch_records_and_reaps_solver() {
        let temp = Temp::new();
        let marker = fake_solver(&temp);
        let manifest=temp.json("cases.json",json!({"mode":"hyperspace","pairs":[[example("X.json"),example("Y.json")],[example("known-X.json"),example("known-Y.json")],[example("X.json"),example("Y.json")]]}));
        let output_file = temp.path("results.jsonl");
        let mut child = Command::new(binary())
            .args([
                "batch",
                manifest.to_str().unwrap(),
                "--engine",
                "z3",
                "--json",
                "--output",
                output_file.to_str().unwrap(),
            ])
            .env("PATH", &temp.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        wait_for_marker(&marker, &mut child);
        assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > Duration::from_secs(4) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("interrupted calculation did not finish");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(130));
        let rows: Vec<Value> = std::fs::read_to_string(output_file)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["status"], "exact");
        assert_eq!(rows[1]["status"], "bounded");
        assert_reaped(&marker);
    }
}
