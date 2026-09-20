# Changes

## 0.2.0 — 2026-09-20

- Add the short `ghdist` command with distance, hyperspace, validate, verify,
  and batch subcommands; preserve the old binary name and legacy flags.
- Accept named JSON distance matrices, exact rational/decimal values, and
  singleton spaces; validate metric axioms and arithmetic ranges explicitly.
- Add concise human output, verbose statistics, structured JSON, checked output
  paths, progress, deadlines, and Unix Ctrl-C handling with retained bounds.
- Extract labelled correspondences from every solver backend. Save standalone
  certificates and verify coverage/distortion through an independent direct
  calculation, keeping lower-bound/optimality evidence separate.
- Add manifest-based batch jobs with per-pair limits, incremental JSONL output,
  input snapshots, and optional per-pair certificates.
- Reorganize the CLI, input, job, runtime, and witness modules behind a shared
  library; add runnable examples and format documentation.
- Add 17 regression tests, including rational precision, certificate tampering,
  interrupted batch persistence, and external-solver cleanup (40 total).
- Preserve the previous source and validation records locally in `history/0.1.0/`
  (excluded from the public package).

## 2026-09-20 — standalone refactoring

- Split the single source file into metric/bitset, bounds, explicit search,
  Hausdorff oracle, generic search, ultrametric recursion, Z3, CLI/configuration,
  and verification modules.
- Use existing certified bounds, eccentricity filtering, and unused-twin symmetry
  reduction in explicit search; avoid copying cached compatibility masks.
- Cache functional constraints and use constant-time star-forest checks in lazy
  search; defer subset eccentricity tables until a backend actually needs them.
- Skip impossible/trivial block allocations and memoize failed remaining-block
  assignments in ultrametric recursion.
- Enforce Z3 variable, clause, and candidate-scan limits during encoding; parse
  exact decision lines and exit status; reap the child after a failed pipe write.
- Reject zero Z3 timeout; mark the optional real-solver test as ignored by default.
- Add independent finite checks and CLI regressions, plus benchmark scripts,
  measured results, and a frozen baseline for comparison.

CLI option names and successful EXACT_RESULT fields remain unchanged. Search
statistics and elapsed times may differ. Resource failures remain errors.
