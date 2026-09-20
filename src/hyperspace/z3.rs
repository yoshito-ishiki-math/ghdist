use super::HausdorffOracle;
use super::generic::RelationPair;
use crate::config::LazyConfig;
use std::fmt::Write as FmtWrite;
use std::io::{Read, Write as IoWrite};
use std::process::{Command, Stdio};

#[derive(Default)]
pub(crate) struct Z3Stats {
    pub(crate) checks: u64,
    pub(crate) variables: usize,
    pub(crate) clauses: u64,
    pub(crate) witness: Vec<RelationPair>,
}

pub(crate) fn feasible(
    left_oracle: &mut HausdorffOracle<'_>,
    right_oracle: &mut HausdorffOracle<'_>,
    left_eccentricities: &[u32],
    right_eccentricities: &[u32],
    epsilon: u32,
    config: &LazyConfig,
) -> Result<(bool, Z3Stats), String> {
    let left_count = left_oracle.point_count();
    let right_count = right_oracle.point_count();
    let mut variables = Vec::new();
    let mut row_variables = vec![Vec::new(); left_count];
    let mut column_variables = vec![Vec::new(); right_count];
    let mut candidate_scans = 0_u64;
    for left in 1..=left_count as u64 {
        for right in 1..=right_count as u64 {
            candidate_scans += 1;
            if candidate_scans.is_multiple_of(1024) {
                crate::runtime::check()?;
            }
            if candidate_scans > config.max_candidate_scans {
                return Err(format!(
                    "Z3 candidate-scan limit {} reached",
                    config.max_candidate_scans
                ));
            }
            if left_eccentricities[left as usize].abs_diff(right_eccentricities[right as usize])
                > epsilon
            {
                continue;
            }
            // Enforce the bound while constructing the encoding, before
            // potentially allocating every pair in the Cartesian product.
            if variables.len() >= config.max_z3_variables {
                return Err(format!(
                    "Z3 encoding exceeded the variable limit {}",
                    config.max_z3_variables
                ));
            }
            let variable = variables.len() + 1;
            variables.push(RelationPair { left, right });
            row_variables[left as usize - 1].push(variable);
            column_variables[right as usize - 1].push(variable);
        }
    }
    if row_variables.iter().any(Vec::is_empty) || column_variables.iter().any(Vec::is_empty) {
        return Ok((false, Z3Stats::default()));
    }
    let mut body = String::new();
    let mut clause_count = 0_u64;
    for clause in row_variables.iter().chain(&column_variables) {
        for variable in clause {
            let _ = write!(&mut body, "{variable} ");
        }
        body.push_str("0\n");
        clause_count += 1;
        if clause_count > config.max_z3_clauses {
            return Err(format!(
                "Z3 encoding exceeded the clause limit {}",
                config.max_z3_clauses
            ));
        }
    }
    for first_index in 0..variables.len() {
        crate::runtime::check()?;
        let first = variables[first_index];
        for (second_index, second) in variables.iter().enumerate().skip(first_index + 1) {
            let left_distance = left_oracle.distance(first.left, second.left);
            let right_distance = right_oracle.distance(first.right, second.right);
            if left_distance.abs_diff(right_distance) > epsilon {
                let _ = writeln!(&mut body, "-{} -{} 0", first_index + 1, second_index + 1);
                clause_count += 1;
                if clause_count > config.max_z3_clauses {
                    return Err(format!(
                        "Z3 encoding exceeded the clause limit {}",
                        config.max_z3_clauses
                    ));
                }
            }
        }
    }

    let mut input = format!("p cnf {} {}\n", variables.len(), clause_count);
    input.push_str(&body);
    let output = run_solver(input, config.z3_timeout_seconds)?;
    let result = parse_output(output.status.code(), &output.stdout, &output.stderr)?;
    let mut witness = Vec::new();
    if result {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some(model) = line.strip_prefix("v ") {
                for token in model.split_whitespace() {
                    let variable = token
                        .parse::<i64>()
                        .map_err(|_| "invalid Z3 model literal")?;
                    if variable > 0 {
                        witness.push(
                            *variables
                                .get(variable as usize - 1)
                                .ok_or("Z3 model variable is out of range")?,
                        );
                    }
                }
            }
        }
    }
    Ok((
        result,
        Z3Stats {
            checks: 1,
            variables: variables.len(),
            clauses: clause_count,
            witness,
        },
    ))
}

fn run_solver(input: String, timeout_seconds: u64) -> Result<std::process::Output, String> {
    crate::runtime::check()?;
    let timeout = format!("-T:{timeout_seconds}");
    let mut child = Command::new("z3")
        .args(["-dimacs", "-in", &timeout])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start optional Z3 backend: {e}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Drain both outputs and write input concurrently. This also lets the main
    // thread enforce the overall deadline while the solver blocks on I/O.
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let errors = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let mut interrupted = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
        if let Err(error) = crate::runtime::check() {
            interrupted = Some(error);
            let _ = child.kill();
            break child.wait().map_err(|e| e.to_string());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let written = writer.join().map_err(|_| "Z3 input thread failed")?;
    let stdout = reader
        .join()
        .map_err(|_| "Z3 output thread failed")?
        .map_err(|e| e.to_string())?;
    let stderr = errors
        .join()
        .map_err(|_| "Z3 error thread failed")?
        .map_err(|e| e.to_string())?;
    if let Some(error) = interrupted {
        return Err(error);
    }
    written.map_err(|e| format!("failed to write Z3 DIMACS input: {e}"))?;
    Ok(std::process::Output {
        status: status?,
        stdout,
        stderr,
    })
}

fn parse_output(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<bool, String> {
    let text = String::from_utf8_lossy(stdout);
    let mut result = None;
    for line in text.lines().map(str::trim) {
        let value = match line {
            "s SATISFIABLE" | "sat" => Some(true),
            "s UNSATISFIABLE" | "unsat" => Some(false),
            "s UNKNOWN" | "unknown" => {
                return Err("Z3 returned unknown; computation incomplete".into());
            }
            _ => None,
        };
        if let Some(value) = value {
            if result.is_some() {
                return Err("Z3 returned multiple result lines".into());
            }
            result = Some(value);
        }
    }
    match result {
        Some(value) if code == Some(0) || code == Some(if value { 10 } else { 20 }) => Ok(value),
        _ => Err(format!(
            "Z3 returned no valid decision (status={code:?}): {}{}",
            text.trim(),
            String::from_utf8_lossy(stderr).trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Metric;

    #[test]
    fn solver_output_requires_an_exact_result_and_matching_exit_status() {
        for (code, text, value) in [
            (0, "s SATISFIABLE\nv 1 0\n", true),
            (10, "s SATISFIABLE", true),
            (0, "s UNSATISFIABLE\n", false),
            (20, "s UNSATISFIABLE", false),
        ] {
            assert_eq!(
                parse_output(Some(code), text.as_bytes(), b"").unwrap(),
                value
            );
        }
        for (code, text) in [
            (Some(1), "s SATISFIABLE"),
            (Some(20), "s SATISFIABLE"),
            (None, "s UNSATISFIABLE"),
            (Some(0), "unknown"),
            (Some(0), "timeout"),
            (Some(0), "error: unsat is not a result"),
            (Some(0), "s SATISFIABLE\ns UNSATISFIABLE"),
        ] {
            assert!(parse_output(code, text.as_bytes(), b"").is_err());
        }
    }

    #[test]
    fn encoding_limits_apply_before_spawning_the_solver() {
        let metric = Metric::from_edges(&[2]).unwrap();
        for (variables, clauses, diagnostic) in
            [(0, 100, "variable limit"), (100, 0, "clause limit")]
        {
            let mut left = HausdorffOracle::new(&metric, 0).unwrap();
            let mut right = HausdorffOracle::new(&metric, 0).unwrap();
            let le = left.subset_eccentricities();
            let re = right.subset_eccentricities();
            let config = LazyConfig {
                max_z3_variables: variables,
                max_z3_clauses: clauses,
                ..LazyConfig::default()
            };
            let error = feasible(&mut left, &mut right, &le, &re, 2, &config)
                .err()
                .unwrap();
            assert!(error.contains(diagnostic), "{error}");
        }
    }
}
