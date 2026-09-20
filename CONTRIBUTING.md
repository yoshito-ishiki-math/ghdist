# Contributing

For a calculation or CLI issue, include:

- `ghdist --version`, OS, Rust version, and Z3 version if used;
- a small pair of JSON inputs and the exact command;
- stdout, stderr, exit code, and whether the result was exact or incomplete;
- the expected result and its mathematical or computational justification.

Remove private paths and unrelated data before sharing input/output files.
An incomplete search is not evidence that a correspondence does not exist.

Use `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
and `cargo test --release --locked`. With Z3 installed, also run
`cargo test --release --locked -- --include-ignored`. Mathematical algorithm
changes should include a small independently checkable regression case.

`PUBLIC-FILES.txt` is an exact allowlist for the source package. If a change
adds a file needed to build, test, or document the tool, update that list and
run `python3 tools/prepare_publication.py --output-dir <new-directory>`.
The output directory must not already exist.
