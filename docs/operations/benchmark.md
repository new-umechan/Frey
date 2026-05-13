# Benchmark

本書は運用文書である。重い科学ベンチマークの目的、実行入口、artifact の置き場、結果の読み方だけをまとめる。
背景説明は `docs/concepts/overview.md`、採用済み仕様の正本は `docs/reference/` を参照する。
日常の壊れ確認と常用ゲートは `docs/operations/test.md` を参照する。

## 目的

- 現実データとのズレを定量化する
- バグとモデル限界を切り分ける材料を残す
- モデル変更やパラメータ調整の判断材料を残す

このベンチマークは quality gate ではない。かなり重いため、手動実行を前提とする。

## artifact の正本

- 比較用 artifact の正本は `benches/results/` に置く
- `docs/operations/bench/` 配下は生成方法、更新条件、判定基準、詳細診断への導線だけを持つ
- 期待値や過去比較の数値そのものは docs ではなく artifact 側を正本とする

## bench の使い分け

### 単体 bench

- Climate: 実地形 + 固定植生で Climate 単体を評価する
- Hydrology: 実地形 + 実気候データで Hydrology 単体を評価する
- Geology: Earth preset 上で tectonics / runtime / hypsometry を診断する
- Ecology: 実気候データ + 実水文データで Ecology 単体を評価する
- Domesticates: 現代分布 proxy と比較して適地モデルを評価する
- Glaciology: 氷厚と海面寄与の診断を行う

### 統合 bench

- Climate + Ecology: 植生フィードバック込みの応答を見る
- Full pipeline: モジュール間相互作用を含む統合評価を行う
- Alpha era transition guard: `Crust -> Environment` 遷移の海陸安定性を診断する

## 主な評価軸

- Phase 1: 代表地域の順序やランキングが現実と整合するか
- Phase 2: 全球スケールの Spearman 相関と分布形状が改善しているか

主指標は Phase 2 の相関と分布形状とする。Phase 1 は悪化理由の切り分けに使う。

## 変数の読み方

- 高信頼: モデル変更の主要判断に使う
- 中信頼: 主要判断に使うが、周辺条件もあわせて読む
- 低信頼: 参考値として保持し、単独では結論に使わない

詳細な変数一覧と判定基準は各 bench 文書を参照する。

## 主な artifact

### perf / scientific

- `tests/scientific-benchmark/scientific-benchmark-samples.json`

### benchmark results

- `benches/results/`
- `benches/results/alpha_transition_guard/alpha_transition_guard.jsonl`

## 詳細文書

### 入口

- `docs/operations/bench/README.md`

### Climate

- `docs/operations/bench/climate/solo.md`

### Hydrology

- `docs/operations/bench/hydrology/solo.md`
- `docs/operations/bench/hydrology/tuning.md`

### Ecology

- `docs/operations/bench/ecology/solo.md`
- `docs/operations/bench/ecology/data_acquisition.md`

### Domesticates

- `docs/operations/bench/domesticates/solo.md`
- `docs/operations/bench/domesticates/data_acquisition.md`

### Geology

- `docs/operations/bench/geology/solo.md`
- `docs/operations/bench/geology/validation_solo.md`
- `docs/operations/bench/geology/validation.md`
- `docs/operations/bench/geology/data_acquisition.md`
- `docs/operations/bench/geology/legacy_hypsometry_handover.md`

### Glaciology / era transition

- `docs/operations/bench/glaciology/solo.md`
- `docs/operations/bench/glaciology/sea_level_series.md`
- `docs/operations/bench/glaciology/data_acquisition.md`
- `docs/operations/bench/glaciology/alpha_transition_guard.md`

## 結果の読み方

- 単体 bench だけ悪化した場合:
    - そのモジュール自体のモデル変更か実装不具合を先に疑う
- 統合 bench だけ悪化した場合:
    - モジュール境界かフィードバック結合を先に疑う
- 低信頼変数だけ悪化した場合:
    - 直ちに gate 扱いせず、参考値として他指標とあわせて読む
- Geology / alpha transition 系で悪化した場合:
    - `docs/operations/bench/geology/` と `docs/operations/bench/glaciology/alpha_transition_guard.md` の診断手順で切り分ける

## 既知の限界

- モンスーン降水の季節性
- 偏西風・貿易風由来の降水非対称性
- 海洋循環の詳細
- 季節的な湖面変動や湿地動態
