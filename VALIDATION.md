# Usability release validation

Date: 2026-09-20T12:08:42+09:00
Agent: Codex
Version: 0.2.0

Scope: named JSON input, exact rational normalization, singleton spaces,
command-line usability, cooperative interruption, correspondence extraction
and independent upper-bound verification, and incremental batch output.
The original Math-Research-GHsp harness was not modified by this release.

Environment: rustc 1.97.1 (8bab26f4f), Cargo 1.97.1 (c980f4866), Z3 4.16.0,
macOS on Apple Silicon. The locked serde_json and Unix libc dependencies were
available in the local cache; all recorded Cargo checks ran offline.

## Commands and results

Run from the standalone project root. An external temporary CARGO_TARGET_DIR
was used during validation.

```sh
cargo fmt --check
cargo clippy --all-targets --offline --locked -- -D warnings
cargo test --offline --release --locked -- --include-ignored
```

Formatting and Clippy passed without warnings. All **40 tests passed**:
28 library tests, 4 legacy CLI integration tests, and 8 usability integration
tests. This includes two real-Z3 tests that are ignored by default when the
optional solver is not installed.

## Newly verified behavior

- Rational strings, decimals, and JSON integers beyond f64 exact precision
  retain their exact values. Both spaces share one integer normalization.
  The two-point lengths 1/3 and 1/2 yield d_GH = 1/12. Singleton/base and
  singleton/hyperspace cases, malformed matrices, and arithmetic overflow
  are covered.
- Explicit, generic, ultrametric, automatic, and Z3 witness paths produce
  correspondences whose coverage and distortion pass the independent
  verifier. Small ultrametric witness cases also match explicit optima.
- Corrupted correspondences and inconsistent declared values fail verification.
  Search metadata cannot make the verifier certify a lower bound or optimality.
- Deadline and search-node exhaustion retain valid intervals without claiming
  an exact distance. An unfinished comparison or witness request remains
  incomplete even when the requested primary distance is exact.
- Human output, verbose details, JSON errors, separate stderr progress, and
  refusal to overwrite existing outputs are checked through real CLI calls.
- Batch paths resolve relative to the manifest. Completed and invalid pair
  records are flushed in order, and saved certificate files can be reverified.
- Unix process tests use a deliberately blocked external solver to verify that
  a global deadline and SIGINT kill/reap the child. SIGINT preserves completed
  batch records and the interrupted pair's interval, stops before the next
  pair, and returns code 130.
- A zero-time verification request returns an incomplete verification record
  with exit code 2, not a successful certificate check.

## Preserved evidence and limits

The prior 23-test refactoring report, its source hashes, and the complete
pre-change standalone folder are preserved in the local history directory
(excluded from the public package).
The archive was checked against all 31 original file hashes. Existing
[benchmarks](benchmarks/README.md) describe that earlier 0.1.0 snapshot;
performance timings were not remeasured for this release.

Tests establish the stated finite regression coverage, not formal verification
or an unrestricted mathematical theorem. Correspondence verification certifies
an upper bound only. The search and certificate sizes remain exponential in
the worst case; deadlines are cooperative. Graceful Ctrl-C preservation is
implemented and tested on Unix, not Windows. Rational intermediate values and
normalized distances have documented checked integer limits.
