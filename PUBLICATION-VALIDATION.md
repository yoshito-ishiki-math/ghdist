# Publication preparation validation

Date: 2026-09-20T13:10:39+09:00
Agent: Codex
Planned repository: `yoshito-ishiki-math/ghdist`
Version: 0.2.0

The GitHub CLI authenticated as `yoshito-ishiki-math`. A read-only repository
lookup returned 404 at preparation time. Name availability must be checked
again when creating the repository. No remote repository, push, or release
was created by this preparation.

## Public source checks

The exact public allowlist contains 59 files, including the benchmark baseline,
all build/test inputs, examples, documentation, and the CI workflow. Local
history archives and original research/Git records are excluded. Exported
provenance omits private-repository commit identifiers. The original MIT
license bytes and copyright notice are preserved.

Checks passed for the public source tree:

- all manifest hashes and local Markdown link targets;
- UTF-8 files and recursively inspected archive contents against the documented
  local-path, private-key, token, and conflict-marker patterns;
- deterministic archive regeneration, including regeneration from the public
  tree itself;
- refusal to overwrite an existing output directory;
- omission of an intentionally added file outside the allowlist;
- refusal of an allowlisted symlink before creating the output;
- refusal of a test local path without echoing its value in the error.

These are scoped inspections, not a guarantee that every possible secret or
sensitive identifier is detectable. The source and allowlist remain reviewable.

## Build and tests

Ran from the exported public tree, using external build directories:

```sh
cargo fmt --check
cargo clippy --all-targets --offline --locked -- -D warnings
cargo test --offline --release --locked -- --include-ignored
```

All 40 Rust tests passed: 28 library tests, 4 legacy CLI tests, and 8 modern
CLI tests, including the two real-Z3 tests. Formatting and Clippy passed with
no warnings. The local environment was macOS, Rust/Cargo 1.97.1, Z3 4.16.0.
Only publication documentation was finalized after this source test; code,
Cargo inputs, test files, and example fixtures were unchanged.

The CI YAML was parsed locally and both jobs inspected. It tests Linux,
macOS, and Windows on stable Rust, plus an exported-source build with Z3 on
Linux. The official checkout action is pinned to commit
`3d3c42e5aac5ba805825da76410c181273ba90b1` (v7), confirmed from GitHub.
Remote CI has not run during local preparation.

See [VALIDATION.md](VALIDATION.md) for the finite computational scope. A passing
test suite does not constitute formal mathematical verification.

## Publication follow-up

Date: 2026-09-20T13:25:04+09:00

The first GitHub run used Rust 1.98 and flagged two manual lowest-set-bit
expressions in Clippy; the Linux exported-source tests with real Z3 passed.
Those expressions now use the equivalent stable `u64::isolate_lowest_one`
method, and Cargo/installation documentation declare Rust 1.97 or newer.
The deadline lifecycle test also waits for the blocked solver to start and
allows process-startup headroom on loaded machines. After both changes, all
40 tests, formatting, and Clippy passed on Rust 1.97.1. Remote checks are
rerun for this correction.
