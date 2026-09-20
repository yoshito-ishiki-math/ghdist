# 性能比較

この記録の「新」は 0.1.0 のリファクタリング後の実装です。0.2.0 の測定ではありません。当時のソース全体は手元に保存しており、公開パッケージには含めていません。以下の手順で現在の実装を使う場合は、新たな比較になります。

2026-09-20、同じマシン・同じ Rust の release ビルドで比較しました。
各 CLI ケースは両バイナリを 1 回ずつ予備実行した後、実行順を交互に替えて 11 回ずつ測定しています。全ての測定で、歪み・GH 距離・空間の大きさ・基礎空間との比較結果が一致しました。

表は中央値（ミリ秒）です。「計算」は入力を構成した後にプログラム内で計測した時間、「全体」はプロセス起動を含む時間です。ごく短い例では起動や測定の揺らぎが支配的であり、全ての入力で高速化するという主張ではありません。

| ケース | 旧・計算 ms | 新・計算 ms | 旧・全体 ms | 新・全体 ms |
| --- | ---: | ---: | ---: | ---: |
| known-base | 0.037 | 0.020 | 2.319 | 2.350 |
| known-explicit-hyperspace | 0.536 | 0.093 | 2.696 | 2.378 |
| known-generic-hyperspace | 1.010 | 0.841 | 3.166 | 2.971 |
| ultrametric-four | 0.015 | 0.015 | 2.143 | 2.138 |
| ultrametric-six | 0.065 | 0.063 | 2.257 | 2.274 |
| identity-twenty | 1.366 | 0.042 | 3.546 | 2.230 |
| optional-z3-five | 78.136 | 77.264 | 81.204 | 80.143 |

元の 12 テストだけを両実装から選び、Z3 テストを含め、テストスレッド数 1 で 3 回ずつ実行した中央値は **11.567 秒 → 0.142 秒** でした。こちらは予備実行なし、プロセス起動を含む時間です。これは同じ有限テスト集合の比較で、一般の入力に対する速度保証ではありません。

四点の既知例の明示的超空間探索では、探索ノード数が 1,303 から 97 に減っています。Z3 の例と小さな超距離専用計算では、今回の変更による時間差は小さいままです。最悪時の指数的計算量と資源上限は引き続き存在します。

- `2026-09-20.json`: 引数、全測定値、出力、バイナリのハッシュ、実行環境。
- `2026-09-20-common-tests.json`: 共通テストの指定、全実行結果、測定値。
- `common-tests.txt`: 比較に使う元の 12 テスト名。
- `baseline-0.1.0.tar.gz`: 変更前のコード・Cargo 設定・ライセンスを保存した凍結アーカイブ。通常の実装として保守・使用するものではありません。

## 再実行

Rust/Cargo、Python 3、Z3 がある環境で、プロジェクトのルートから実行します。Python は標準ライブラリだけを使用します。Z3 のない環境では `benchmark.py` に `--skip-z3` を付けてください。

```sh
bench_dir=$(mktemp -d)
tar -xzf benchmarks/baseline-0.1.0.tar.gz -C "$bench_dir"
cargo build --release --offline --manifest-path "$bench_dir/ghsp-rust-exact-search-baseline/Cargo.toml" --target-dir "$bench_dir/baseline-target"
cargo build --release --offline
python3 tools/benchmark.py --baseline "$bench_dir/baseline-target/release/ghsp-rust-exact-search" --candidate target/release/ghsp-rust-exact-search --output "$bench_dir/results.json" --repetitions 11
```

共通テストの再測定には `tools/benchmark_suite.py` を使います。両側で `cargo test --release --offline --no-run` を実行して表示された単体テスト実行ファイルを、それぞれ `--baseline` と `--candidate` に指定し、`--output` に保存先を渡してください。比較対象に Z3 テストを含むので、こちらは Z3 が必要です。

変更前のソースハッシュは `../PROVENANCE.json`、現在のソースハッシュは `../SOURCE-MANIFEST.json` にあります。コンパイラやハードウェアが異なれば、バイナリのハッシュと所要時間も変わります。
