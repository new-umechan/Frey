# Subsistence Module Proposal

## Status

Accepted

## Goal

`Subsistence` モジュールを、単なる環境適合 proxy ではなく、
資源 access・技術能力・人口圧・移動性・貯蔵・リスク分散を通じて
生業配分と食料供給特性を導くモデルとして再定義し、
実装に移れる粒度で責務と公開 state を固定する文書である。

## Reason

従来案は、

- 局所環境から生業 mix がほぼ決まる
- 漁撈を内水面と沿岸で分けない
- 牧畜の移動性を持たない
- 人口圧による intensification を持たない
- 供給安定性を資源安定性と十分に分離していない

という点で、簡略モデルとしては動いても、
考古学・人類学・人間生態学の知見に照らすと説明力が不足していた。

本 proposal は、v1 で実装可能な範囲を保ちつつ、
学術的に外しにくい因果を明示的に取り込む。

## Scope

この proposal で決めること:

- `Subsistence` が何を読むか
- `Subsistence` が何を書くか
- モジュール内部の system 分割
- `SubsistenceMix` の軸と意味
- `food` 系 state をどう分解するか
- `surface_water_access` の責務移管
- 定住・人口側へ何を渡すか
- `Ecology` feedback の範囲
- v1 で必須とする学術的因果

この proposal でまだ決めないこと:

- 各式の最終係数
- 地域別の詳細パラメタ
- 高解像の沿岸生産性モデル
- 社会制度・交易・文化選好の完全導入
- ベンチ設計
- `reference` への昇格タイミング

## Design Decision

### 1. `Subsistence` は Tier 1 module として維持する

`Subsistence` は Tier 1 module として残す。

理由:

- 資源 access、能力、戦略更新、供給特性、土地利用圧は強く結びつく
- `Population` / `Settlement` / `Ecology` との接続点を一箇所で固定できる
- 実装初期段階で module を細分化するより、module 内の system 分割の方が妥当である

### 2. 生業は「環境適合」ではなく「制約下の戦略選択」として扱う

`Subsistence` は各セルの生業を、
局所環境から直接決まる値としてではなく、
次の制約と能力のもとで更新される戦略配分として扱う。

- 資源への access
- 家畜化・栽培化・漁撈・貯蔵・移動の能力
- 人口圧による intensification 圧力
- 供給平均と供給変動の trade-off
- 混合戦略による risk reduction

### 3. 水アクセスは `Hydrology` の責務に移す

- `freshwater_access` は廃止する
- `Hydrology` が `surface_water_access` を書く
- `Population` / `Settlement` は `Hydrology.surface_water_access` を読む
- `Subsistence` は `surface_water_access` を読むが書かない

`surface_water_access` は、
飲用・生活用水・基礎的生業で利用可能な表流水 access の proxy である。
灌漑能力や地下水利用の完全代理ではない。

### 4. 生業表現は 5 軸を維持する

公開 state の `SubsistenceMix` は 5 軸を維持する。

```rust
struct SubsistenceMix {
    gathering:   f32,
    hunting:     f32,
    fishing:     f32,
    cultivation: f32,
    herding:     f32,
}
```

各フィールドは `0.0..=1.0`、合計は `1.0` とする。

理由:

- `gathering` / `hunting` / `fishing` は access 条件も risk 構造も異なる
- 漁撈は定住化・貯蔵・季節性と強く結びつきうるため独立軸が必要
- `cultivation` と `herding` は intensification と mobility の性質が異なる

### 5. ただし `fishing` は内部的に内水面と沿岸を分ける

公開 state は 1 軸 `fishing` のままとするが、
内部では少なくとも次を区別する。

- `inland_aquatic_access`
- `coastal_aquatic_access`

理由:

- 河川・湖沼依存と沿岸依存では、資源集中、季節性、定住性、技術要件が異なる
- 学術的には両者を同一 access で潰すと説明を歪めやすい

### 6. `food_production` は廃止し、供給平均・供給変動・buffer に分ける

単一の `food_production` は採用しない。
`Subsistence` は少なくとも次を公開する。

- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`

必要に応じて下流で `food_stability` を合成してよいが、
正本は平均・変動・buffer の分解表現とする。

理由:

- 平均供給量と供給変動は別物である
- 供給安定性は資源の年変動だけでなく、貯蔵・移動・混合戦略で改善する
- 定住維持や人口ショック耐性は、平均供給だけでなく buffer の有無に左右される

## Proposed Model

### 公開 state

`Subsistence` は次を公開する。

- `subsistence_mix`
- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `mobility_capacity`
- `land_use_intensity`

`Hydrology` は次を公開する。

- `surface_water_access`

### 公開 state の意味

#### `subsistence_mix`

各セルがどの獲得戦略にどれだけ依存しているかを表す配分である。
活動量そのものではなく、食料獲得依存の比率である。

#### `food_energy_mean`

- `0.0..=1.0` の正規化 proxy
- 当該セルが平均的に確保しうる食料供給余力
- `Population` の人口支持力の主入力

#### `food_energy_variance`

- `0.0..=1.0` の正規化 proxy
- 当該セルの供給変動性
- 高いほど供給は不安定
- `Population` の死亡率ショック、`Settlement` の脆弱性計算に使う

#### `buffer_capacity`

- `0.0..=1.0` の正規化 proxy
- 貯蔵、乾燥・燻製・発酵などの保存、家畜在庫、季節間持越しの総合 proxy
- `food_energy_variance` の影響を緩和する
- 定住維持に強く効く

#### `mobility_capacity`

- `0.0..=1.0` の正規化 proxy
- 季節移動、放牧移動、採捕レンジ拡張により局所変動を回避できる程度
- 牧畜・採集・狩猟・一部漁撈の安定化に寄与する

#### `land_use_intensity`

- `0.0..=1.0` の正規化 proxy
- 当該セルでの土地利用強度
- `Ecology` への pressure 算出に使う
- 農耕 intensification や高密度利用を反映する

### `SubsistenceMix` の各軸

- `gathering`
  野生植物採集への依存
- `hunting`
  野生動物狩猟への依存
- `fishing`
  内水面・沿岸を含む水産資源利用への依存
- `cultivation`
  栽培を主とする食料生産への依存
- `herding`
  家畜飼養・放牧を主とする食料生産への依存

### 学術的に v1 で必須とする因果

v1 では少なくとも次を入れる。

1. 資源 access と供給能力を分ける
2. 平均供給と供給変動を分ける
3. 貯蔵・移動・混合戦略で安定性が改善する
4. 人口圧が intensification を押す
5. 漁撈は内水面と沿岸を内部で分ける
6. 牧畜の安定性には mobility を効かせる
7. 定住性は `cultivation` だけでなく、`buffer_capacity` と tethered resource に依存する

## Module Responsibilities

### `Subsistence` が決めるもの

- 各セルの生業依存配分 (`subsistence_mix`)
- 平均供給 proxy (`food_energy_mean`)
- 供給変動 proxy (`food_energy_variance`)
- buffer proxy (`buffer_capacity`)
- mobility proxy (`mobility_capacity`)
- 土地利用強度 (`land_use_intensity`)
- `Ecology` に返す人為圧

### `Subsistence` が決めないもの

- 表流水 access (`surface_water_access`)
- 人口変動そのもの
- 集落形成そのもの
- 国家形成
- 交易ネットワークの完全モデル
- 制度・政治・文化選好の完全モデル

## System Breakdown

`Subsistence` は少なくとも次の system で構成する。

1. `AccessSystem`
2. `CapabilitySystem`
3. `StrategySystem`
4. `OutputSystem`
5. `PressureSystem`

### 1. `AccessSystem`

役割:

- 環境条件から各戦略の資源 access を導出する

読むもの:

- `Hydrology`
    - `river_flow`
    - `is_lake`
    - `surface_water_access`
- `Ecology`
    - 植生
    - 地被
    - 土壌 fertility
- `WorldProjection`
    - `is_coastal`
    - `distance_from_ocean`

書くもの:

- `wild_plant_access`
- `wild_animal_access`
- `inland_aquatic_access`
- `coastal_aquatic_access`
- `arable_potential`
- `grazing_potential`
- `seasonality`
- `interannual_variability`

補足:

- `fishing` は内部的に内水面と沿岸を分ける
- `seasonality` と `interannual_variability` は供給変動の基礎制約である
- ここではまだ技術能力や人口圧を入れない

### 2. `CapabilitySystem`

役割:

- 家畜化・栽培化・貯蔵・移動の能力を導出する

読むもの:

- `Domesticates`
    - `crop_adoption`
    - `livestock_adoption`
- `Hydrology`
    - `surface_water_access`
    - `river_flow`
- `WorldProjection`
    - `is_coastal`
- 前 tick の `subsistence_mix`

書くもの:

- `cultivation_capacity`
- `herding_capacity`
- `fishing_capacity`
- `storage_potential`
- `mobility_capacity_raw`

補足:

- 能力は access とは別物である
- `storage_potential` は保存加工・在庫化可能性の proxy
- `mobility_capacity_raw` は季節移動・放牧移動の実行可能性の proxy

### 3. `StrategySystem`

役割:

- access・capability・人口圧・前 tick 状態から `subsistence_mix` を更新する

読むもの:

- 前 tick の `subsistence_mix`
- `AccessSystem` の派生量
- `CapabilitySystem` の派生量
- `Population`
    - `population`

書くもの:

- `subsistence_mix`
- `intensification_pressure`

判断原理:

- 各生業軸の期待収益と期待リスクを評価する
- `population` から局所の `population_pressure` を導出する
- 人口圧が低いときは広域・低強度戦略が残りやすい
- 人口圧が高いときは `cultivation` や高強度利用への遷移圧が上がる
- ただし adoption や環境制約がない場合は遷移しない
- 混合戦略は risk reduction を通じて選好されうる
- 牧畜は `mobility_capacity_raw` が高いと変動環境でも維持されやすい

更新則:

- 前 tick の mix から target mix へ緩和する
- 最後に合計 1.0 へ正規化する
- 全軸魅力度が 0 の場合は前 tick を保持する

### 4. `OutputSystem`

役割:

- 確定した `subsistence_mix` と各派生量から、
  人口・定住側が読む供給特性を導出する

読むもの:

- `subsistence_mix`
- `AccessSystem` の派生量
- `CapabilitySystem` の派生量
- `intensification_pressure`

書くもの:

- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `mobility_capacity`
- `land_use_intensity`

導出原理:

- `food_energy_mean` は各軸の期待供給の混合平均
- `food_energy_variance` は季節性・年変動・単一依存・脆弱 access で増える
- `buffer_capacity` は保存可能資源、栽培 surplus、家畜在庫、定着資源で増える
- `mobility_capacity` は狩猟・採集・牧畜の変動緩和に効く
- `land_use_intensity` は `cultivation` 比率だけでなく人口圧と intensification を反映する

### 5. `PressureSystem`

役割:

- 生業配分と利用強度から `Ecology` への pressure を導出する

読むもの:

- `subsistence_mix`
- `land_use_intensity`
- `population`
- 必要に応じて `surface_water_access`

書くもの:

- `logging`
- `grazing`
- `anthropogenic_fire`
- `cultivation_pressure`
- `nutrient_pressure`

補足:

- pressure は mix だけでなく利用強度で決まる
- v1 でも `Population.population` は読む
- これにより、同じ農耕比率でも人口密度が違えば圧力が変わる

## Inputs

`Subsistence` は次を読む。

- `Hydrology`
    - `surface_water_access`
    - `river_flow`
    - `is_lake`
- `Ecology`
    - 植生
    - 地被
    - 土壌 fertility
- `Domesticates`
    - `crop_adoption`
    - `livestock_adoption`
- `Population`
    - `population`
- `WorldProjection`
    - `is_coastal`
    - `distance_from_ocean`
- 自身の前 tick 状態
    - `subsistence_mix`

### 読まないもの

- 国家
- 交易量の正本値
- 文化圏・宗教圏・制度の完全表現

### 気候入力について

気候の生値は直接読まない。
気候影響は `Hydrology` / `Ecology` の公開値を通じて受ける。

これは、`Subsistence` を環境統合の下流 module として保つための設計判断である。
ただし将来、季節性 proxy が上流から十分に来ない場合は再検討する。

## Downstream Changes

### `Population`

`Population` は次を読む。

- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `surface_water_access`

利用契約:

- `food_energy_mean` が高いほど人口支持力は下がらない
- `food_energy_variance` が高いほど供給ショック脆弱性は下がらない
- `buffer_capacity` が高いほど変動影響は緩和される

### `Settlement`

`Settlement` は次を読む。

- `subsistence_mix`
- `food_energy_mean`
- `food_energy_variance`
- `buffer_capacity`
- `mobility_capacity`
- `surface_water_access`

利用契約:

- 定住成立は `cultivation` のみでなく、
  `fishing`、`buffer_capacity`、`surface_water_access`、`mobility_capacity` の組み合わせでも起こりうる
- 高い `food_energy_mean` と低い有効変動は集約化を支える
- 高い `mobility_capacity` は一部の牧畜・採捕社会で定住圧を弱めうる

## Ecology Feedback

`Subsistence` は `Ecology` に次の pressure を送る。

- `logging`
- `grazing`
- `anthropogenic_fire`
- `cultivation_pressure`
- `nutrient_pressure`

### 生業ごとの主な圧力

- `gathering`
    - 基本は低圧
    - 条件により弱い `anthropogenic_fire` / `logging`
- `hunting`
    - 植生圧は低い
    - 条件により弱い `anthropogenic_fire`
- `fishing`
    - 陸上植生圧は低い
    - ただし沿岸・河畔利用に伴う局所圧は将来拡張余地を残す
- `cultivation`
    - `cultivation_pressure`
    - `nutrient_pressure`
    - 条件により `anthropogenic_fire` / `logging`
- `herding`
    - `grazing`
    - 条件により弱い `anthropogenic_fire`

## Compatibility Notes

本 proposal は現行の `reference` と暫定実装を大きく変更する。

主な変更点:

- `food_production` を廃止する
- `freshwater_access` を廃止し、`Hydrology.surface_water_access` に置き換える
- `food_energy_mean` / `food_energy_variance` / `buffer_capacity` を新設する
- `mobility_capacity` / `land_use_intensity` を公開 state に追加する
- `StrategySystem` が `Population.population` を読む
- `PressureSystem` が `Population.population` を読む
- `farming` / `pastoralism` は `cultivation` / `herding` に改名する

採用時に更新が必要なもの:

- `docs/reference/architecture/data_model.md`
- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/modules/subsistence.md`
- `docs/reference/modules/hydrology.md`
- `docs/reference/modules/ecology.md`
- `Population` / `Settlement` / `Subsistence` の実装

## Acceptance Criteria

この proposal が固まった状態とは、implementer が次を迷わず答えられる状態を指す。

- `Subsistence` が何を読むか
- `Subsistence` が何を書くか
- 学術的に v1 で外さない因果は何か
- 内部 system は何に分かれるか
- `fishing` を内部でどう分けるか
- `food_energy_mean` / `food_energy_variance` / `buffer_capacity` の役割差は何か
- `Population` / `Settlement` が各指標をどういう役割で読むか
- `Ecology` への feedback が mix だけでなく利用強度に依存すること

## Open Items

次段階で別途決める。

- `population_pressure` の具体式
- `buffer_capacity` の具体的合成式
- `mobility_capacity` の具体的合成式
- 沿岸 access の近似方式
- `food_energy_variance` を `Settlement` がどう閾値化するか
- 混合戦略ボーナスの大きさ
- 交易・文化・制度をどの段階で導入するか

## Research Basis

この proposal は少なくとも次の一般的知見と整合する方向を採る。

- aquatic adaptation は独立の説明軸を持つべきである
- agricultural intensification には risk management と人口圧が関与する
- sedentism は高収量だけでなく、貯蔵・資源集中・技術・人口圧と結びつく
- pastoralism は非平衡環境に対する mobility を本質に持つ

採用時には `docs/decisions/` に、
どこまでを v1 の学術的必須因果とし、
どこから先を将来拡張とするかを短く固定する。

採用判断:

- `docs/decisions/260508-subsistence-model-foundations.md`
