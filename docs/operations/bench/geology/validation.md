# Geology 検証運用

本書は `geology` の長期検証 bench をどう実行し、何を読んで、どこまでを pass/fail の判断材料にするかをまとめる運用文書である。
Geology の実装仕様は `docs/reference/`、設計変更の意図は `docs/proposal/` と `docs/decisions/` を参照する。
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
