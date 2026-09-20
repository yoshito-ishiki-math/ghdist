# ghdist v0.2.0

Initial standalone public release candidate for exact finite
Gromov–Hausdorff and Hausdorff hyperspace computations.

- Named JSON distance matrices, exact fractions/decimals, and singleton spaces.
- Human output, JSON, validation, progress, time limits, and retained bounds.
- Correspondence certificates from all backends, with independent coverage
  and distortion verification.
- Incremental JSONL batches and optional per-pair certificate files.
- Optimized explicit, generic, ultrametric, and optional Z3 search engines.
- Legacy command compatibility, runnable examples, and Japanese/English guides.

The local macOS suite passed all 40 Rust tests including the two optional
real-Z3 tests. Formatting and Clippy passed. GitHub CI is configured separately;
its remote results should be checked after the repository is published.

Correspondence verification establishes an upper bound. Search remains
exponential in the worst case; bounded or interrupted results are explicitly
incomplete. The software is not formally verified.

The underlying engine was used in Yoshito Ishiki's *AI-Assisted Research
Report: Finite Hausdorff Hyperspaces and Gromov–Hausdorff Geometry*, version 1.0,
DOI: [10.5281/zenodo.21913552](https://doi.org/10.5281/zenodo.21913552).
This report DOI is separate from the software release.
