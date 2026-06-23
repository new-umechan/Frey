# Geology 検証運用

本書は `geology` の長期検証 bench をどう実行し、何を読んで、どこまでを pass/fail の判断材料にするかをまとめる運用文書である。
Geology の実装仕様は `docs/reference/`、設計変更の意図と検討状態は `docs/decisions/` を参照する。
Earth 実データ比較ベンチは `docs/operations/bench/geology/solo.md` を参照する。

## 目的

- 長期 Plate 挙動の破綻を早めに見つける
- Earth 類似の地殻構造が維持されているかを確認する
- 海面依存の見かけの変化と、地殻構造自体の破綻を切り分ける

## 使いどころ

- Geology の主要ロジックを変更したとき
- 主要パラメータを変更したとき
- `alpha_transition_guard` や hypsometry 系の異常を長期挙動まで遡って確認したいとき

## 前提

- 本 bench の主対象は海面そのものではなく、地殻の由来・年齢・境界過程である
- `land_ratio` や海岸線長は補助診断として読む
- `crust_type` が sea level 由来の派生量に戻っていないことを前提とする

## 主な評価軸

### 必須で見るもの

1. 大陸地殻と海洋地殻が共存していること
2. 大陸地殻と海洋地殻で高度分布が分離していること
3. 若い海洋地殻ほど浅く、古い海洋地殻ほど深いこと
4. 海嶺から離れるほど海洋地殻年齢が増加すること

### 補助で見るもの

- `land_ratio`
- 海岸線長
- 浅海面積
- hypsometry の崩れ方

## 実行タイミング

- 自動検証:
    - `cargo test`
    - `debug_assert!`
- 定量評価:
    - モデル・主要パラメータ変更時
- 定性評価:
    - 定量評価を通したあとに手動で読む

## Pre-plate emergence の確認

Damage-first 初期化を変更したときは、長期 bench の前に pre-plate emergence を先に確認する。
目的は、Crust runtime まで進める前に「弱い境界 network + 強い内部 block」が
初期化段階で成立しているかを切り分けることにある。

最低限、次を読む。

1. `plate_emergence_probe`
2. `crust_plate_count_series` の tick 0

`plate_emergence_probe` では `selected_valid_count` / `selected_regime` だけでなく、
`evolution_iterations[]` の checkpoint 推移を読む。
見るべき点は次である。

- base budget 前後で mobile-lid 候補がいつ立つか
- `selected_valid_count` が伸びたあと、追加 step で崩れていないか
- `settled_steps` が毎回 hard cap 側へ張り付き続けていないか
- `selected_multi_component_plate_count` が 0 を維持しているか
- `selected_mean_detached_fragment_ratio` が 0 近傍か
- `selected_final_plate_count` が target 帯から大きく外れていないか
- `selected_max_plate_area_ratio` が突出していないか
- `selected_effective_plate_count` が低すぎないか
- `selected_mean_plate_boundary_complexity` / `selected_max_plate_boundary_complexity` が
  seed 間で突出していないか

最後の 5 つは、plate 数が妥当でも shape や面積分布が現実離れしていないかを見るための補助指標である。
特に `multi_component_plate_count == 0` なのに complexity だけが高い場合は、
plate が分断されているのではなく、境界が細かく蛇行している問題と読む。
逆に `selected_valid_count` が十分でも `selected_max_plate_area_ratio` が高く
`selected_effective_plate_count` が低い場合は、
複数 block が見えていても最終 `plate_id` では少数の plate が支配している問題と読む。
この場合は score penalty を足す前に、
threshold candidate 群の中に area balance の良い候補が存在するかを先に確認する。

`crust_plate_count_series` は tick 0 だけでも、初期 plate field が runtime 前に
何 plate へ compact されたかを確認できる。
runtime まで見る場合は `plate_count` だけでなく、
`plate_id_churn_rate`、`orphan_cell_count`、`single_cell_plate_count` も合わせて読む。
plate 数が維持されていても `single_cell_plate_count` が増えるなら、
runtime ownership transfer が degenerate micro-plate を作っている可能性がある。

### Plate motion naturalness の確認

Crust runtime の plate が「形を保っているが、500万年/tick のスケールで動いていない」
ように見える場合は、`crust_plate_count_series` の motion 指標を読む。

例:

```bash
env CRUST_PLATE_SERIES_SEED=alpha \
CRUST_PLATE_SERIES_TICKS=120 \
CRUST_PLATE_SERIES_RECORD_EVERY=10 \
CRUST_PLATE_SERIES_BENCH_OUT=/tmp/frey_crust_plate_series.jsonl \
cargo run --manifest-path rust/Cargo.toml --bin crust_plate_count_series
```

主要列だけ読む:

```bash
jq -r '.samples[] | [
  .tick,
  .mean_plate_speed_km_per_myr,
  .max_plate_speed_km_per_myr,
  .mean_cell_crossing_fraction_per_tick,
  .mean_direction_persistence,
  .reciprocal_churn_ratio,
  .mean_centroid_path_straightness
] | @tsv' /tmp/frey_crust_plate_series.jsonl
```

読み方:

- `mean_plate_speed_km_per_myr`
    - 現実の典型値は `20-100 km/Myr`、高速側で `100-150 km/Myr`
    - late Crust で `1 km/Myr` 未満へ落ちる場合は、5 Myr/tick に対して遅すぎる
- `mean_cell_crossing_fraction_per_tick`
    - 1 tick の移動量が平均セル間隔の何倍か
    - level 依存なので、同じ level の run 同士で比較する
- `mean_direction_persistence`
    - sample 間の plate velocity 方向 cosine
    - `0.7` 未満が続く場合は jitter / random walk を疑う
- `reciprocal_churn_ratio`
    - `from -> to` と `to -> from` の相互取り合いが少ないほど 1 に近い
    - 0 に近い場合は境界のせめぎ合いを疑う
- `mean_centroid_path_straightness`
    - `net displacement / cumulative path length`
    - 低いほど往復や蛇行が多い

この指標は pass/fail gate ではなく、plate motion の自然さを読む診断 artifact とする。

2026-06-23 の basal motion floor 導入後、alpha level 6 の 80 tick run では
`mean_plate_speed_km_per_myr` が tick 20 以降も `42-63 km/Myr` に残った。
導入前は tick 40 以降で `0.1 km/Myr` 前後まで落ちていたため、
late Crust の速度崩れは解消したと読む。
同じ run で `mean_cell_crossing_fraction_per_tick` は `1.7-2.6` 程度なので、
次に見るべき点は複数セル相当の displacement と ownership transfer の整合である。

自動化可能な配列整合性、値域、決定性、snapshot 整合はコード側で担保する。
本書では手動サニティチェック手順を保持しない。

## 結果の読み方

- 大陸地殻と海洋地殻の共存が崩れた場合:
    - 地殻生成や変換ロジックの破綻を先に疑う
- 高度分布の分離だけ崩れた場合:
    - hypsometry か isostatic / smoothing 系の悪化を疑う
- age-depth 傾向だけ崩れた場合:
    - 海洋地殻の冷却・沈降や年齢更新の破綻を疑う
- ridge age gradient だけ崩れた場合:
    - 海嶺近傍の生成・更新順序や境界分類を疑う
- 補助診断だけ悪化した場合:
    - 海面依存の変化か、主評価軸の悪化に引きずられた副作用かを切り分ける

## 関連文書

- `docs/operations/bench/geology/solo.md`
- `docs/operations/bench/geology/validation_solo.md`
- `docs/operations/bench/geology/legacy_hypsometry_handover.md`
- `docs/operations/bench/glaciology/alpha_transition_guard.md`

## 学術アンカー

- USGS, water coverage
- NOAA NCEI, hypsographic curve / ETOPO
- Stein & Stein (1992), oceanic age-depth
- NOAA NCEI, ocean crust age dataset

閾値調整や指標見直しが必要になった場合は、まず上記アンカーと現在の artifact を照合して判断する。
