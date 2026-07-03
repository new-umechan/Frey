# Test

本書は運用文書である。日常開発で使うテスト手順とゲート基準だけをまとめる。
背景説明は `docs/concepts/overview.md`、採用済み仕様の正本は `docs/reference/` を参照する。
重いベンチマーク運用は `docs/operations/benchmark.md` を参照する。
用語の正本は `docs/reference/terminology.md` を参照する。

## 目的

- 日常変更のあとに破壊的な回帰がないかを素早く確認する
- seed 固定の回帰を継続監視する
- 常用ゲートと補助ゲートの役割を分ける

## 用語

- test: 日常開発で回す自動確認
- gate: pass / fail で変更の受け入れ可否を決める自動判定
- benchmark: 現実データや artifact と比較してモデルの性質を読む重い評価
- bench: コマンド、パス、短い識別子で使う benchmark の略称

この文書では、日常実行と CI 判定を扱う。
科学モデルの妥当性確認や重い artifact 比較は `docs/operations/benchmark.md` に置く。

## 日常実行

### Rust ユニットテスト

- コマンド: `cargo test`
- 使いどころ:
    - 日常の変更確認
    - リファクタ後の安全確認
    - PR 前の最低限チェック

### seed 回帰ゲート

- 常用経路は Rust native runner とし、WASM build を必須にしない
- コマンド:

```sh
pnpm test:seed:gate:quick
```

```sh
pnpm test:seed:gate:heavy
```

- 役割:
    - `test:seed:gate:quick`: 毎回回す軽量ゲート
    - `test:seed:gate:heavy`: PR 前の重ゲート

### 補助ゲート

- WASM 互換確認が必要なときだけ使う
- コマンド:

```sh
pnpm test:seed:gate:quick:wasm
```

```sh
pnpm test:seed:gate:heavy:wasm
```

```sh
pnpm test:gate:regression:wasm
```

### perf ゲート

- `native + wasm + worker` の 3 レーンを常設する
- コマンド:

```sh
pnpm bench:perf:gate
```

```sh
pnpm bench:perf:gate:native
```

```sh
pnpm bench:perf:gate:wasm
```

```sh
pnpm bench:perf:gate:worker
```

### ScientificBenchmark artifact 更新

```sh
pnpm bench:scientific:samples
```

### wasm API テスト

```sh
cd rust && wasm-pack test --node
```

## seed 回帰ゲート基準

### 比較指標

- `land_cells`
- `land_ratio`
- `height_mean`
- `height_std`
- `max_river_flux`
- `top10_river_flux_sum`

### 閾値

- 共通閾値: `0.005`
- `top10_river_flux_sum` のみ: `0.01`
- `land_ratio` absolute guard:
    - warning: `0.24 - 0.35`
    - fail: `0.20 - 0.40`

### 実行条件

- `test:seed:gate:quick`: 4 seeds x 16 ticks x 1 run
- `test:seed:gate:heavy`: 8 seeds x 24 ticks x 1 run
- `test:seed:gate:quick` / `test:seed:gate:heavy` は `--jobs 2` を使う

### baseline

- `tests/seed-regression/seed-regression-quick-baseline.json`
- `tests/seed-regression/seed-regression-heavy-baseline.json`

### baseline 整合チェック

- `--check` 時は `meta.ticks` / `meta.level` / `meta.seeds` が baseline と一致しない場合に差分レポートへ記録する
- あわせて `meta.transition_mode` / `meta.era_boundaries` / `meta.eras_at_measurement` の不一致も差分レポートへ記録する

## perf / benchmark artifact

### perf baseline

- `tests/perf/bench-baseline-native.json`
- `tests/perf/bench-baseline-wasm.json`
- `tests/perf/bench-baseline-worker.json`

### ScientificBenchmark sample

- `tests/scientific-benchmark/scientific-benchmark-samples.json`

## CI 自動実行

- heavy seed gate: `.github/workflows/seed-regression-heavy-gate.yaml`
- perf gate: `.github/workflows/perf-gate.yaml`
- ScientificBenchmark artifact: `.github/workflows/scientific-benchmark-artifact.yaml`
- wasm API tests: `.github/workflows/wasm-api-tests.yaml`

## 手動確認

- 表示崩れ
- 時代切替 UI
- 再生 / 停止 / 巻き戻し
- 介入時の挙動

## 失敗時の見方

- Rust unit test が落ちた場合:
    - ロジック回帰を先に疑う
- native seed gate だけ落ちた場合:
    - シミュレーション更新か baseline の不整合を確認する
- WASM 補助ゲートだけ落ちた場合:
    - transport / wasm 経路の差分を確認する
- perf gate が落ちた場合:
    - どのレーンで落ちたかを切り分けて baseline と比較する
- ScientificBenchmark が悪化した場合:
    - 直ちにバグと断定せず、`docs/operations/benchmark.md` の基準でモデル変更か実装不具合かを切り分ける
