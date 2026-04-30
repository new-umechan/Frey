# Geology 堆積・侵食 Earth ベンチ v1 提案

## Status

Superseded by `geology-benchmark-split-and-hydrology-sediment-ownership.md`

## 背景

- 現行の `docs/operations/benchmark.md` では、Geology は Plate の定量検証を中心に扱っており、現代地球の堆積・侵食傾向に対する実データ比較ベンチは未定義である。
- 一方で `docs/proposal/sediment-mass-conserving-land-balance.md` と `docs/decisions/260422-exner-sediment-balance-and-subsidence.md` により、`erosion_rate`・`deposition_rate`・sediment export を質量収支付きで扱う方針は固まった。
- しかし現状は「収支が破綻していないか」は見えても、「現代地球に対して侵食傾向と地形条件付き堆積がどれだけ妥当か」を定量測定する科学ベンチがない。
- Geology の堆積・侵食は高解像度観測と 1 セル約 100 km のモデル解像度の差が大きく、PASS/FAIL の quality gate にするとモデル限界と実装退行を切り分けにくい。

## 目的

- `geology_solo` を、現代地球の実データに対して堆積・侵食傾向を比較する v1 科学ベンチとして再定義する。
- v1 では長期地形差分ではなく、「現代の傾向場」をどれだけ再現できているかを評価する。
- `docs/proposal/sediment-mass-conserving-land-balance.md` で導入した sediment budget 制約が、Earth 条件で妥当な空間パターンにつながるかを比較可能にする。
- ベンチ結果を合否ではなく、最新値・過去 baseline・差分で読む運用に統一する。
- 主要河川 outlet やデルタ hotspot の整合は Hydrology 側の downstream transport 検証へ分離し、Geology は terrain response と budget 診断に集中する。

## 提案概要

### 1. ベンチの位置づけ

- v1 の `geology_solo` は quality gate ではなく、モデル妥当性を定量測定する重い手動ベンチとする。
- seed は `earth` 固定、`mesh_level=6` 固定とし、現代地球の地形・気候・水文入力を使う。
- 実装・運用の型は既存 `hydrology_solo` に寄せ、比較可能な JSONL artifact を `benches/results/` に蓄積する。

### 2. 評価対象

- 主評価は fluvial erosion / deposition の空間傾向と terrain-conditioned な堆積配分に置く。
- v1 では多数 tick 後の地形差分、層序、氷河起源 sediment transport、長期 basin infill は対象外とする。
- 河口 outlet・デルタ hotspot・主要河川ランキングは Geology v1 の対象外とし、Hydrology 側の downstream transport benchmark で扱う。
- 絶対値一致ではなく、順位相関・地形条件付き share・収支診断を主にみる。

### 3. 入力条件

- 地形入力は NOAA ETOPO 2022 を正本とし、既存 `benches/raw/geology/ETOPO_2022_v1_60s_N90W180_surface.tif` を使う。
- 気候・水文入力は既存の Earth ベンチ資産を再利用する。
  Hydrology 入力は `benches/data/hydro_input.bin` または同等の ERA5-Land 由来 runoff を使う。
  地形参照は `benches/data/terrain_ref.bin` を使う。
- ベンチ中は実地形を固定入力として扱い、侵食・堆積の妥当性評価に集中する。

### 4. 空間解像度の扱い

- モデルの 1 セルは `mesh_level=6` で約 100 km スケールである。
- GloSEM や HydroSHEDS のような高解像度データは、このセルへ集約した比較量として使う。
- 集約はセル重心の単純サンプルではなく、可能な限り面積加重平均または coverage 加重集計を使う。
- 小流域、狭いデルタ、細い海岸線は v1 では under-resolved と明示し、評価は macro pattern を優先する。

## 主指標

### Phase 2 主指標

- `erosion_rate_spearman`
  GloSEM 等の土壌侵食参照とモデル `erosion_rate` のセル単位順位相関。
  絶対量ではなく空間順位の一致を見る。

- `sediment_budget_ratio`
  `Σdeposition / Σerosion` を基本とし、open-boundary export を併記して mass balance の診断量として使う。

- `coastal_deposition_share`
  海岸・浅海セルへの堆積比率。堆積の多くが内陸高地へ誤配分されていないかを確認する粗い診断量として使う。

- `low_slope_deposition_share`
  低勾配セルへどれだけ堆積が集まるかをみる。急斜面へ堆積が張り付く退行を検出する。

### 補助診断

- `erosion_reference_coverage`
- `open_boundary_export_fraction`
- `lake_deposition_share`

主指標が動いた理由を掘り下げる用途で使い、v1 の主比較値は上記 4 指標に絞る。

## 参照データ要件

### 必須

- 標高入力: NOAA ETOPO 2022
  https://www.ncei.noaa.gov/products/etopo-global-relief-model
- 侵食参照: JRC/ESDAC GloSEM
  https://esdac.jrc.ec.europa.eu/themes/global-soil-erosion

### データ運用方針

- GloSEM は土壌侵食モデル由来であり、Frey の地形侵食の直接観測ではない。
  したがって v1 は絶対値比較を主目的にせず、順位相関を主評価とする。
- 主要河川 sediment yield や outlet 比較は Hydrology 側の downstream transport benchmark で扱う。
- 高解像度入力はベンチ前処理で `mesh_level=6` セルへ集約し、元データそのものの解像度再現は要求しない。

## 出力フォーマット要件

既存 `hydrology_solo` 型にそろえ、1 run ごとに JSONL 1 行を出す。

- `schema_version`
- `bench`
- `seed`
- `mesh_level`
- `cell_count`
- `runtime`
- `phase2.metrics`
- `diagnostics`

最低限の形は次を想定する。

```json
{
    "schema_version": 1,
    "bench": "geology_solo",
    "seed": "earth",
    "mesh_level": 6,
    "cell_count": 40962,
    "runtime": {
        "geology_step_p50_ms": 0.0,
        "geology_step_p95_ms": 0.0,
        "stabilization_ticks": 12,
        "sample_ticks": 10
    },
    "phase2": {
        "state": "ready",
        "metrics": {
            "erosion_rate_spearman": 0.0,
            "sediment_budget_ratio": 0.0,
            "coastal_deposition_share": 0.0,
            "low_slope_deposition_share": 0.0
        }
    },
    "diagnostics": {
        "open_boundary_export_fraction": 0.0,
        "erosion_reference_coverage": 0.0,
        "lake_deposition_share": 0.0
    }
}
```

## 欠損データ時の扱い

- 必須参照が欠けても bench 全体を即失敗させない。
- 比較不能なときは `phase2.state = "skipped"` とし、欠損理由を `diagnostics` に残す。
- 一部指標だけ計算可能な場合は `phase2.state = "ready"` を維持しつつ、欠損した個別指標を `null` または欠損理由つきで記録する。
- runtime と基本収支診断は、参照データが一部なくても可能な限り残す。

## 成功条件

- 実装者が `pnpm run bench --suite geology_solo` で Earth 条件の geology benchmark を再実行できる。
- 出力は合否ではなく、比較に必要な JSONL artifact と比較レポートとして保存できる。
- `docs/proposal/sediment-mass-conserving-land-balance.md` の sediment budget 制約と整合する指標群を持つ。
- Hydrology 側へ移した downstream transport 検証と責務分離されている。
- データ欠損時にも「なぜ比較できなかったか」が artifact から追跡できる。

## スコープ

この proposal で決めること:

- `geology_solo` v1 の目的
- Earth 固定入力と実データ比較の前提
- 主指標と補助診断
- JSONL artifact の最小要件
- 欠損データ時の state 運用

この proposal でまだ決めないこと:

- GloSEM の具体的前処理スクリプト詳細
- 長期地形変化を扱う v2 指標
- geology quality gate の閾値

## リスクとトレードオフ

- 100 km セルではデルタや峡谷の細部を失うため、局所再現性は低い。
- GloSEM は土壌侵食 proxy であり、露岩侵食・氷河侵食・海底輸送を十分には表さない。
- `coastal_deposition_share` と `low_slope_deposition_share` は coarse-grained な proxy であり、個別デルタの再現性までは保証しない。
- ただし長期地形差分をいきなり主評価にするより、現代 Earth の傾向場比較から入る方が運用コストと解釈性のバランスがよい。

## 実施計画

1. 本 proposal を採択し、`docs/operations/benchmark.md` の Geology 節に Earth ベンチ方針を反映する。
2. `docs/operations/bench/geology/` に `solo.md` または同等の実行・データ取得手順文書を追加する。
3. `geology_solo` を `hydrology_solo` 型の JSONL 出力へ移行する。
4. GloSEM 集約の前処理を実装する。
5. `pnpm run bench:run:geology-series -- --runs 5` と `pnpm run bench:compare:geology` を整備し、比較 artifact を `benches/results/` に保存する。

## 検証計画

- 文書: `pnpm docs:check`
- 単発実行: `pnpm run bench --suite geology_solo`
- 系列実行: `pnpm run bench:run:geology-series -- --runs 5`
- 比較レポート: `pnpm run bench:compare:geology`

品質評価は PASS/FAIL ではなく、最新値・baseline・差分を並べて読む運用にする。

## 既存 proposal との関係

- `docs/proposal/sediment-mass-conserving-land-balance.md` は、sediment budget を非発散化するモデル側の提案である。
- 本 proposal は、その設計が Earth 条件でどれだけ妥当な erosion / deposition pattern を作るかを測定するベンチ要件である。
- したがって両者は競合ではなく、前者がモデル設計、後者が科学ベンチ設計を担当する。

## 未解決事項

- GloSEM をどの空間集約法で 100 km セルへ落とすか
- coastal / shallow marine の境界定義をどう固定するか
- `sediment_budget_ratio` に glacial export をどこまで含めるか
- `low_slope_deposition_share` の勾配閾値を固定値とするか、相対閾値にするか

## 参考

- `docs/proposal/sediment-mass-conserving-land-balance.md`
- `docs/decisions/260422-exner-sediment-balance-and-subsidence.md`
- Borrelli et al., GloSEM global soil erosion assessments
