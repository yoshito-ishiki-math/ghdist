# ghdist

[日本語の説明](README.md)

A research CLI for exact Gromov–Hausdorff distances between finite metric
spaces and between their Hausdorff hyperspaces of nonempty subsets.

It accepts named JSON distance matrices, rational and decimal distances, and
singleton spaces. It can export correspondences, independently verify their
distortion, retain bounds when interrupted, and process batches of pairs.

## Install and try

Install Rust 1.97 or newer and Cargo, then run from a source checkout:

```sh
cargo install --path . --locked --bin ghdist
ghdist distance examples/X.json examples/Y.json
# X ↔ Y [base]: d_GH = 1/12 (exact)
ghdist validate examples/point.json
ghdist hyperspace examples/X.json examples/Y.json --compare-base
```

Put Cargo's binary directory (usually `~/.cargo/bin`) on PATH. Without
installing, use `cargo run --release -- distance ...`. Dependencies are locked
in `Cargo.lock`; offline builds work after they have been cached. Z3 is an
optional external solver and must be on PATH when explicitly selected.

## Input and output

```json
{
  "name": "X",
  "points": ["a", "b"],
  "distances": [[0, "1/3"], ["1/3", 0]]
}
```

`name` and `points` are optional. Rational strings and decimal JSON numbers are
parsed exactly, without conversion to binary floating point. Metric axioms are
checked. A singleton is `{"distances": [[0]]}`; empty spaces are unsupported.

Add `--json` for structured output, `--verbose` for statistics, or
`--output result.json` to save a result. Existing output files are never
overwritten. See [the format reference](docs/FORMATS.md) for schemas and limits.

```sh
ghdist hyperspace examples/known-X.json examples/known-Y.json --progress --time-limit 30s
ghdist hyperspace examples/ultrametric-X.json examples/ultrametric-Y.json --certificate witness.json
ghdist verify witness.json
ghdist batch examples/batch.json --output results.jsonl
```

Progress goes to stderr. Deadlines and resource limits leave certified lower
and upper bounds and an incomplete status. Unix Ctrl-C also preserves these
bounds and completed batch records, and stops the running external solver.
Graceful Ctrl-C preservation is not implemented on Windows. Deadlines are
cooperative rather than hard wall-clock guarantees.

`verify` recomputes coverage and distortion independently of the search
engines. **It verifies an upper bound, not the reported lower bound or
optimality.** Requested witness or comparison work can be incomplete even
when the main distance is already exact. Exit codes are 0 for completion,
1 for input/I/O errors, 2 for incomplete work, and 130 for Unix Ctrl-C.

Batch paths are relative to the manifest; each pair is flushed to JSONL as it
finishes. Supported engines are `auto`, `explicit`, `lazy-generic`,
`ultrametric`, and `z3`. The old `ghsp-rust-exact-search` binary and legacy
edge-list flags remain available.

Search can be exponential. Lazy hyperspace inputs support at most 20 base
points; witnesses have a default limit of 10,000 pairs. Exact arithmetic uses
checked u128 intermediates and u32 normalized distances. These representation
limits and the available search budgets are described in `ghdist --help` and
the format reference.

## Research use

The computation engine underlying this tool was used for concrete examples in
the following report. At the time it ran inside the Math-Research-GHsp harness;
the standalone `ghdist` CLI was subsequently extracted and improved.

Yoshito Ishiki, *AI-Assisted Research Report: Finite Hausdorff Hyperspaces and
Gromov–Hausdorff Geometry*, version 1.0, Zenodo, 13 August 2026.
DOI: [10.5281/zenodo.21913552](https://doi.org/10.5281/zenodo.21913552).

The DOI identifies the report, not a software release. The PDF, source archive,
and publication records are on the author's
[research reports page](https://yoshito-ishiki-math.github.io/ai-research-reports.html).

## Development and license

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --locked
# With Z3 installed:
cargo test --release --locked -- --include-ignored
```

See [VALIDATION.md](VALIDATION.md) for the finite test coverage and
[CONTRIBUTING.md](CONTRIBUTING.md) for useful issue reports. CI is configured
for Linux, macOS, and Windows, with a separate Linux Z3/source-package job.
The recorded local checks ran on macOS; configured CI jobs are not evidence
that those remote runs have already passed.

MIT licensed; the original copyright notice is retained in [LICENSE](LICENSE).
Public source provenance and hashes are in `PROVENANCE.json` and
`SOURCE-MANIFEST.json`. The historical [benchmarks](benchmarks/README.md)
describe the earlier 0.1.0 optimization, not new 0.2.0 timing measurements.
