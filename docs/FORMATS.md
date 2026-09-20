# JSON formats (version 1)

## Space

A space is an object with `distances` (a nonempty square matrix), optional `name`,
and optional `points` (unique, nonempty string names in matrix order). Missing
point names default to `"0"`, `"1"`, … . Unknown space fields are rejected.

Distances are nonnegative JSON numbers or strings containing an integer,
nonnegative decimal/scientific notation, or an integer fraction `p/q`, with
`q > 0`. Diagonal entries must be zero and all other entries positive.
Symmetry and the triangle inequality are checked exactly. A singleton has
`"distances": [[0]]`; empty spaces and pseudometrics are not supported.

Decimals are parsed from their decimal spelling, without conversion to f64.
Both inputs share a common denominator and common-factor reduction. Arithmetic
is checked in u128 and normalized distances must fit u32; unrepresentable
inputs fail rather than being rounded. Validation includes this representation
check. Input files are limited to 64 MiB and matrices to 1,024 points.

## Calculation result

`distance` and `hyperspace` return one JSON object with `--json`.
`--output` saves the same result as pretty-printed JSON. Each result includes:

- `schema_version`, `solver_version`, `layer`, requested `engine`;
- `inputs`: complete input spaces, with distances written as rational strings;
- `left`, `right`: display names and base point counts;
- `status`: `exact` or `bounded`;
- `d_gh`, `distortion`: exact rational strings when complete, otherwise `null`;
- `lower_bound`, `upper_bound`: certified bounds on d_GH, in input units;
- `lower_bound_evidence`: description of the solver's lower-bound argument;
- `reason`: why an incomplete calculation stopped;
- `normalization_unit`: original length of one internal integer distance unit;
- `base_comparison`: optional exact distance or interval for the base spaces;
- `stats`: search counters, or `null` if the search did not complete;
- `elapsed_ms`: elapsed computation time, including requested witness work;
- `certificate_status`: `not-requested`, `available`, or `incomplete`.

`--witness` adds `certificate` to this object; `--certificate` saves it separately
and adds `certificate_file`. An exact distance can coexist with an unfinished
base comparison or witness request. Exit code 2 signals that additional work
was incomplete. Resource exhaustion never means infeasibility.

Progress is emitted only to stderr. `--time-limit` starts after input validation
and pair normalization, includes base comparison and witness extraction, and
is checked cooperatively. It is per pair in batch mode. File serialization is
not part of the cooperative search deadline. Z3 is monitored while its stdin,
stdout, and stderr are handled concurrently, and is killed/reaped on cancellation.
Unix SIGINT produces a bounded record where possible and exit code 130.

Invalid input/I/O produces `{"schema_version":1,"status":"error","message":...}`
with `--json` and exit code 1. Interrupted certificate verification produces
`status: "incomplete"`, `verification_completed: false`, and exit code 2 (130
for SIGINT).

## Correspondence certificate

A certificate has `schema: "ghdist.correspondence.v1"`, `layer: "base"` or
`"hyperspace"`, complete `left` and `right` spaces, and `correspondence`:

```json
[
  {"left": ["a"], "right": ["u"]},
  {"left": ["b"], "right": ["v"]}
]
```

For the base layer, every endpoint has exactly one name. In the hyperspace
layer, an endpoint is a nonempty subset represented by distinct names. The
relation must cover every point of each compared space, including every
nonempty subset in the hyperspace layer.

`distortion` is the maximum distance discrepancy for this relation;
`upper_bound` is half of it, both in original rational units. `search_report`
is separate solver metadata, including its reported lower bound and status.

`verify` checks the metric inputs, coverage, actual distortion, and consistency
of the declared upper bound. Its direct Hausdorff max–min computation is
independent of the search oracle. A successful check returns
`status: "verified-upper-bound"` and `optimality_verified: false`. It neither
trusts nor verifies the search report's lower bound. Even when both reported
bounds agree, this command alone does not certify optimality.

Correspondence generation and verification have a configurable pair limit
(default 10,000), with at most 20 base points for hyperspace witnesses. Checking
all relation pairs is quadratic in certificate size, with additional work for
Hausdorff distances. A requested witness may hit limits after the distance has
already been determined; that exact value is retained.

## Batch manifest and JSONL

Provide exactly one of `spaces` or `pairs`:

```json
{"spaces": ["X.json", "Y.json", "point.json"], "mode": "distance"}
```

```json
{"pairs": [["X.json", "Y.json"], ["X.json", "Z.json"]], "mode": "hyperspace"}
```

Relative paths resolve against the manifest directory. `spaces` expands in
input order to index pairs `i < j`; `pairs` preserves the given order.
`mode` defaults to `distance`. `batch --hyperspace` overrides it. Inputs are
read once into a snapshot before pair computations begin.

Each line of a batch result file is a complete calculation result or input
error, with `pair_index` (one-based), `left_file`, and `right_file` added.
Every line is flushed before the next pair. Invalid pairs do not prevent later
pairs from running. On SIGINT, the current partial result is saved and remaining
pairs are not started. The final completion count goes to stderr.

`--certificate-dir` writes `pair-000001.json`, etc. Existing result and
certificate paths are rejected; there is no implicit overwrite or resume.
