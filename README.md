# ghdist

[English quick start](README.en.md)

有限距離空間の Gromov–Hausdorff 距離と、非空部分集合全体に Hausdorff 距離を入れた超空間の距離を厳密に計算する研究用 CLI です。名前付き距離行列、有理数、一点空間、対応の出力・検証、一括計算に対応します。

## 導入

Rust/Cargo が必要です。Rust 1.97.1、macOS で検証しています。このフォルダで次を実行すると、短いコマンド名 `ghdist` を利用できます。

```sh
cargo install --path . --locked --bin ghdist
```

Cargo のインストール先（通常 `~/.cargo/bin`）を PATH に含めてください。インストールせずに試す場合は `cargo run --release -- distance ...`、またはビルド後の `target/release/ghdist` を使えます。

JSON の正確な読み書きに `serde_json`、Unix の Ctrl-C 処理に `libc` を使用します。依存バージョンは `Cargo.lock` に固定しています。初回は依存の取得が必要で、キャッシュがあれば `--offline` でも動きます。Z3 は任意の外部ソルバーで、使う場合だけ `z3` を PATH に置きます。

## まず試す

このフォルダで実行します。

```sh
# 1/3 と 1/2 の二点空間: d_GH = 1/12
ghdist distance examples/X.json examples/Y.json

# 入力検査。一点空間も使えます
ghdist validate examples/point.json

# 超空間と基礎空間を比較
ghdist hyperspace examples/X.json examples/Y.json --compare-base

# 点名付きの対応を保存し、別の検証器で確認
ghdist hyperspace examples/ultrametric-X.json examples/ultrametric-Y.json --certificate witness.json
ghdist verify witness.json

# 複数の空間を全ペア比較し、完了した結果から順次保存
ghdist batch examples/batch.json --output results.jsonl
```

`witness.json` と `results.jsonl` は新しいファイル名を指定してください。既存のファイルは上書きしません。

## 入力ファイル

```json
{
  "name": "X",
  "points": ["a", "b"],
  "distances": [[0, "1/3"], ["1/3", 0]]
}
```

`name` と `points` は省略できます。点名は重複しない文字列、距離は JSON の数値または有理数の文字列です。`"1/3"`、`0.5`、`"0.125"`、`1e-3` を正確に読み取ります。二進浮動小数への丸めは行いません。

一点空間は `{"distances": [[0]]}` です。空集合は対象外です。行列の大きさ、対称性、対角成分、正値性、三角不等式を検査し、誤りの位置を報告します。

左右の行列を同じ倍率で整数化し、計算結果を元の単位に戻します。内部計算のため、共通分母などの中間値は `u128`、共通因子で約分した整数距離は `u32` の範囲に収まる必要があります。範囲を超える場合は丸めずにエラーにします。入力は 64 MiB、距離行列は 1,024 点までです。探索の上限はさらに別に適用されます。

詳しい形式と結果の項目は [docs/FORMATS.md](docs/FORMATS.md) にあります。

## 結果・進捗・中断

```sh
ghdist distance examples/known-X.json examples/known-Y.json --verbose
ghdist hyperspace examples/known-X.json examples/known-Y.json --json --output result.json
ghdist hyperspace examples/known-X.json examples/known-Y.json --time-limit 30s --progress
```

通常は GH 距離と計算状態を簡潔に表示します。`--verbose` で探索統計などの詳細を、`--json` で機械可読な結果を出力します。分数は `"1/12"` のような正確な文字列です。結果 JSON には入力行列も保存します。

`--progress` は経過時間と確定済みの上下界を標準エラー出力へ表示し、標準出力の JSON に混ぜません。`--time-limit` は入力検証後の計算・基礎空間との比較・対応の復元を合わせた制限です。`500ms`、`30s`、`2m` を指定できます。バッチでは各ペアに個別に適用します。安全な中断点で確認するため、厳密な実時間の上限ではありません。

時間切れ・探索上限では、確定した上下界を `status: "bounded"` として残し、`d_gh` は `null` にします。Unix では Ctrl-C でも同様に処理し、実行中の Z3 を終了・回収します。Windows での Ctrl-C の保存動作は提供していません。保存済みのバッチ行はそのまま残ります。

| 終了コード | 意味 |
| --- | --- |
| 0 | 要求した計算・検証が完了 |
| 1 | 入力または入出力のエラー |
| 2 | 探索・比較・対応の復元・検証が未完了 |
| 130 | Ctrl-C による中断（Unix） |

距離が確定していても、追加指定した基礎空間の比較や対応の復元が未完了なら終了コード 2 になります。確定した距離は結果内に保持します。

## 対応の出力と検証

`--certificate FILE` で対応を独立したファイルへ保存し、`--witness --json` で結果 JSON に埋め込めます。明示的探索、一般の遅延探索、超距離専用探索、Z3 の各方式に対応します。超空間の点は、元の点名の配列で表示します。

`ghdist verify FILE` は入力行列から全域性と歪みを再計算します。Hausdorff 距離は定義の max–min 式で計算し、探索用の距離キャッシュを使いません。**この検証が確認するのは上界です。** 最適性と下界は探索の記録として別に保存し、この検証だけで確認済みとはしません。

対応は指数的な大きさになることがあります。既定の上限は 10,000 組で、`--max-witness-pairs` で変更できます。検証は組数の二乗に比例する比較を含みます。出力や検証にも `--time-limit` を使えます。対応を取り出せなかった場合は、値だけの計算結果と理由を残します。

## 一括計算

```json
{"spaces": ["X.json", "Y.json", "point.json"], "mode": "distance"}
```

パスはマニフェストのあるフォルダから解釈します。`spaces` は異なる添字の全ての組 `i < j` を列挙します。`pairs` で比較する組を指定する形式も使えます。

```sh
ghdist batch examples/hyperspace-batch.json --json --output hyperspaces.jsonl --certificate-dir certificates
```

入力を読み込んでから順番に計算し、結果を 1 ペア 1 行で保存・flush します。入力エラーのペアも記録し、残りを続けます。Ctrl-C では進行中のペアの上下界を記録して停止します。再開機能ではないため、再実行時は新しい出力ファイル名を使ってください。

## 探索方式と従来の使い方

`--engine` は `auto`（既定）、`explicit`、`lazy-generic`、`ultrametric`、`z3` から選べます。基礎空間の計算には明示的探索を使います。`auto` は構造による判定と適した探索を組み合わせ、任意のソルバーが使えない場合は対応可能な方式へ切り替えます。

最悪時の計算量は指数的です。n 点空間の超空間は `2^n - 1` 点あります。遅延表現は基礎空間 20 点まで対応しますが、その全入力を短時間で解けるという意味ではありません。`ghdist --help` に探索量・メモリ・Z3 の各制限を掲載しています。

従来の `--left-edges`、`--known-pair`、`--self-test` などの呼び出しと `EXACT_RESULT` 出力も保持しています。旧名のバイナリ `ghsp-rust-exact-search` もビルドできます。

```sh
ghdist --known-pair --hyperspace --engine auto --compare-base
```

## 開発・検証

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --locked
# Z3 がある場合、実ソルバーを使う任意のテストも含める
cargo test --release --locked -- --include-ignored
```

[VALIDATION.md](VALIDATION.md) に検証範囲、[CHANGELOG.md](CHANGELOG.md) に変更点を記録しています。テストの成功は形式検証や一般定理の証明ではありません。

`src/lib.rs` が共通実装、`app.rs` / `input.rs` / `job.rs` がファイル入出力と計算の組立て、`runtime.rs` が進捗・中断、`witness.rs` が対応と独立検証です。距離空間・上下界・各探索方式は別モジュールに分けています。公開 Rust API はまだ提供しておらず、CLI と JSON を入口とします。

## 研究での利用

本ツールの元となった計算機構は、以下の AI-Assisted Research Report における具体例の計算に使用されました。報告書作成時に Math-Research-GHsp ハーネス内で使用した実装を独立した CLI として切り出し、改良したものが現在の `ghdist` です。

Yoshito Ishiki, *AI-Assisted Research Report: Finite Hausdorff Hyperspaces and Gromov–Hausdorff Geometry*, version 1.0, Zenodo, 13 August 2026. DOI: [10.5281/zenodo.21913552](https://doi.org/10.5281/zenodo.21913552).

報告書の PDF・ソース・公開記録は、著者ホームページの [AI-Assisted Research Reports](https://yoshito-ishiki-math.github.io/ai-research-reports.html) に掲載しています。

## 由来とライセンス

Math-Research-GHsp から独立して切り出した計算コードです。元の MIT ライセンスと著作権表示を `LICENSE` に保持しています。切り出し時の記録は `PROVENANCE.json`、現在のソースハッシュは `SOURCE-MANIFEST.json` にあります。

[benchmarks/](benchmarks/README.md) の性能記録は 0.1.0 のリファクタリング時の測定です。最初のコードは比較用の凍結アーカイブとして同梱しています。旧版全体の保存記録は手元に保管し、公開パッケージには含めません。公開対象と手順は [PUBLICATION.md](PUBLICATION.md) に記載しています。
