# GitHub publication preparation

- Planned repository: `yoshito-ishiki-math/ghdist`
- Visibility: public
- Description: Exact finite Gromov–Hausdorff distances and Hausdorff hyperspace computations.
- Version: `0.2.0`; proposed release tag: `v0.2.0`
- License: MIT, retaining the existing copyright notice without modification.
- Release notes: [docs/RELEASE-NOTES-0.2.0.md](docs/RELEASE-NOTES-0.2.0.md)

This document prepares publication. Building the package performs no GitHub
writes, commits, pushes, or uploads. The report DOI in the README identifies
the research report, not this software version.

## Prepare a reviewable source package

The exporter requires Python 3.11 or later and only its standard library.

```sh
python3 tools/prepare_publication.py --output-dir .publication/v0.2.0
```

The output contains `ghdist/` (the GitHub-ready source tree), a deterministic
`ghdist-0.2.0.tar.gz`, `SHA256SUMS`, and a local preparation report. It refuses
to overwrite an existing directory and copies only exact paths listed in
`PUBLIC-FILES.txt`. Files needed by tests, example data, both Cargo manifests,
documentation, and the CI workflow are included.

Local history archives, original research records, Git history, generated
results, and build products are excluded. Public provenance is derived from
the extraction hashes; private-repository commit identifiers are omitted.
The public `SOURCE-MANIFEST.json` is regenerated for the exported files.
The small original benchmark baseline is included and inspected recursively.

The exporter checks file types, local-path/credential patterns, and archive
members. These checks have a defined scope and do not replace reviewing the
listed public files. Build and test the exported tree before publication:

```sh
cd .publication/v0.2.0/ghdist
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --locked -- --include-ignored
```

The last command requires Z3. Without Z3, omit `-- --include-ignored`.
The latest local run used Rust 1.97.1 and Z3 4.16.0 on macOS. GitHub's CI
workflow is configured for Linux/macOS/Windows and a Linux Z3/package job;
remote CI remains to be checked after the first push.

Local preparation evidence is recorded in [PUBLICATION-VALIDATION.md](PUBLICATION-VALIDATION.md).

## Publish when instructed by the owner

Use the GitHub CLI account `yoshito-ishiki-math`; check `gh api user --jq .login`
before any external write. If the repository name already exists, inspect it
before proceeding. The prepared tree may already have a local Git index;
review `git status` and `git diff --cached` before committing.

From the exported source tree, the intended sequence is:

```sh
# Initialize only if the exported tree has no .git directory yet.
git init --initial-branch=codex/prepare-publication
git add --all
git diff --cached --check
git commit -m "Prepare ghdist v0.2.0 for public use"
gh repo create yoshito-ishiki-math/ghdist --public --source . --remote origin --description "Exact finite Gromov-Hausdorff distances and Hausdorff hyperspace computations"
git push -u origin HEAD:main
gh repo edit yoshito-ishiki-math/ghdist --default-branch main
```

Verify the repository contents and CI runs before creating a release. The
prepared release-note file can be passed to `gh release create` with
`--notes-file` when a release is requested. Do not reuse the report's DOI as a
software DOI. GitHub source publication does not require crates.io publication;
`publish = false` remains in Cargo.toml.

Workflow references:
[GitHub's Rust CI guide](https://docs.github.com/en/actions/tutorials/build-and-test-code/rust),
[official checkout action](https://github.com/actions/checkout), and
[GitHub CLI repository creation](https://cli.github.com/manual/gh_repo_create).
