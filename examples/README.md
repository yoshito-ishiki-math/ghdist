# Examples

Run from the project root after installing `ghdist`.

| Input | Meaning |
| --- | --- |
| `X.json`, `Y.json` | Two-point spaces of lengths 1/3 and 1/2; d_GH = 1/12 |
| `point.json` | A singleton; its distance to X is 1/6 |
| `known-X.json`, `known-Y.json` | Original four-point regression pair; base and hyperspace d_GH = 242 |
| `ultrametric-X.json`, `ultrametric-Y.json` | Isometric ultrametrics with different labels; base and hyperspace distance 0 |
| `batch.json` | All three pairs among X, Y, and the singleton |
| `hyperspace-batch.json` | Two selected hyperspace comparisons |

```sh
ghdist validate examples/X.json examples/Y.json examples/point.json
ghdist distance examples/X.json examples/Y.json
ghdist hyperspace examples/known-X.json examples/known-Y.json --compare-base --json
ghdist hyperspace examples/ultrametric-X.json examples/ultrametric-Y.json --engine ultrametric --certificate witness.json
ghdist verify witness.json
ghdist batch examples/hyperspace-batch.json --output results.jsonl --certificate-dir certificates
```

Output names must not already exist. Add `--progress --time-limit 30s` when
exploring larger inputs. The numerical examples are regression checks, not
unrestricted mathematical statements.
