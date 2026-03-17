# Test

本書はテスト運用メモである。
設計の正本は `docs/architecture/overview.md` と `docs/architecture/data_model.md` を参照する。

## 目的

- 大きな変更のあとに「壊れていないか」を素早く確認する
- seed固定の回帰確認を行う
- 将来の `World` ベース回帰テストの指標を先に定義しておく

## テストの層（運用）

### 1. Rustユニットテスト（現在の主力）

- `cargo test` を実行する
- 地形生成、河川、侵食の基礎ロジックの回帰を確認する

使いどころ:

- 日常の変更確認
- リファクタ後の安全確認
- PR前の最低限チェック

### 2. シミュレーション回帰テスト（段階導入）

固定seedを複数用意し、指標を出力して前回結果と比較する。
「完全一致」ではなく、許容変動幅つきで比較する運用を想定する。

現時点では、手動スクリプトまたは補助コマンドでの実行を想定。
将来的に `World` 実装後は自動化する。

#### seed固定回帰CLIの運用ルール（2026-03-17）

- 実行は `npm run seed:regression:dev -- ...` を基本とする
- `npm run seed:regression -- ...` はWASMを自動ビルドしないため、古い生成物を参照する可能性がある

ゲート運用:

1. 常時ゲート（毎回）
2. 重ゲート（PR前）

採用している比較指標:

- `land_cells`
- `height_mean`
- `height_std`
- `max_river_flux`
- `top10_river_flux_sum`

許容誤差の決定手順:

1. 条件を固定して実測する（12 seeds x 5 runs, 32 tick, level=6）
2. run1をbaselineとし、run2-5の差分を集計する
3. 各指標で `abs(current - baseline) / abs(baseline)` を計算する（baselineが0のときは絶対差）
4. 各指標のP95に安全余白 `+0.005` を加算する
5. 小数第4位で切り上げて最終閾値とする

実測結果（2026-03-17, サンプル数48/指標）:

- 全5指標で差分の `min=max=p95=0`
- よって最終閾値は全指標 `0.005`

推奨コマンド例:

```sh
npm run seed:gate:quick
```

```sh
npm run seed:gate:heavy
```

ゲート条件:

- `seed:gate:quick`: 4 seeds x 16 ticks x 1run
- `seed:gate:heavy`: 8 seeds x 24 ticks x 1run

baselineファイル:

- `testdata/seed-regression/seed-regression-quick-baseline.json`
- `testdata/seed-regression/seed-regression-heavy-baseline.json`

baseline誤用防止:

- `--check`時に `meta.ticks` / `meta.level` / `meta.seeds`（順序無視の集合）がbaselineと一致しない場合は即FAILにする

自動化:

- 重ゲートは `.github/workflows/seed-regression-heavy-gate.yml` で次の契機で自動実行する
  - `pull_request`
  - `push` to `main`
  - `workflow_dispatch`（手動実行）

#### perfベースラインゲート（2026-03-17）

- `scripts/bench.mjs` の `--baseline` / `--threshold` をCIで常時実行する
- baselineファイルは `testdata/perf/bench-baseline.json`
- コマンド:

```sh
npm run perf:gate
```

自動化:

- `.github/workflows/perf-gate.yml` で次の契機で自動実行する
  - `pull_request`
  - `push` to `main`
  - `workflow_dispatch`（手動実行）

#### wasm APIテスト（2026-03-17）

- `tick/restore/checkpoint` を含むwasm APIテストは `wasm-pack test --node` で実行する
- コマンド:

```sh
cd rust && wasm-pack test --node
```

自動化:

- `.github/workflows/wasm-api-tests.yml` で次の契機で自動実行する
  - `pull_request`
  - `push` to `main`
  - `workflow_dispatch`（手動実行）

### 3. 手動確認（UI/統合）

- 表示崩れ
- 時代切替UI
- 再生/停止/巻き戻し（実装後）
- 介入時の挙動（実装後）

## 現在すぐ測れる指標（地形・河川）

今の主対象は地形・河川。

- 陸地セル数
- 標高の平均・標準偏差
- 陸地の最大連結成分サイズ（最大大陸の大きさの近似）
- 大陸数（面積閾値以上の連結成分数）
- 河川流量の最大値
- 河川流量上位10本の合計

補足:
将来的には、河川オートマトンのステップ数を固定した状態で比較する。

## 将来追加する指標（環境形成期以降）

### 環境形成期が安定してきたら

- 主要河川の数（流量閾値以上）
- 河川の総延長（セル数ベース）
- 堆積量の総量（陸上 / 河口 / 浅海）
- 河川流量変化量の移動平均

### 気候実装後

- 気温場の平均・分散
- 降水場の平均・分散
- 可住域割合（暫定閾値）

### 文明実装後

- 最初の定住発生tick
- 文明数（国家数または政治単位数）
- 総人口
- 都市数

## `World` ベース回帰テストの目標API（予定）

`World` 実装後に、次のようなAPIで回帰テストを書ける状態を目標にする。

```rust
let mut world = World::new(seed, params);
world.run(n_ticks);
let metrics = world.metrics();
```

初版では、完全なAPIがなくてもよい。
まずは `step_world()` を一定回数回し、指標を取得できれば十分。

## スナップショット運用（将来）

- JSONまたはバイナリで指標スナップショットを保存する
- `git diff` で前回との差分を確認できる形にする
- フル状態の保存ではなく、まずは指標スナップショットから始める

## 実行タイミング（目安）

- 「壊れてないか心配なとき」
- 大きな変更のあと
- サブシステム接続（河川 -> 気候 -> 生態 -> 文明）の節目
- 時代スケール制御の変更後

## API受け入れ観点（Checkpoint / Layer / Tick）

実装時の受け入れ確認として、次を最低ラインにする。

1. `save_checkpoint` 実行後、`list_checkpoints` に新規idが追加される
2. `load_checkpoint` 実行後、tickとeraが保存時点へ戻る
3. 不正idで `load_checkpoint` を呼んだ場合、例外になる
4. `get_field` は既知kindに対してFloat32Arrayを返す
5. 未生成レイヤーkindと不正kindは例外になる
6. `tick()` は `step(1)` ごとに単調増加する
7. `tick()` は時代名ではなく累積管理Tickカウンタを返す

## 地形非回帰の受け入れ観点（現行）

1. 同一seedで生成した初期heightを保存し、十分なTick進行後のheightと比較して「初期値へ単調回帰」していないこと
2. 長時間進行時の地形変化が、地形サブシステム増分更新と河川侵食オートマトンの合成として説明できること
