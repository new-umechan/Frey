# Subsistence Module Proposal

## Status

Draft

## Goal

`Subsistence` モジュールの責務、内部 system 構成、公開 state、
および `Hydrology` / `Ecology` / `Domesticates` / `Population` / `Settlement`
との接続点を、実装に移れる粒度で固定する文書である。

## Scope

この proposal で決めること:

- `Subsistence` が何を読むか
- `Subsistence` が何を書くか
- モジュール内部の system 分割
- `SubsistenceMix` の軸と意味
- `food` 系 state の役割
- `surface_water_access` の責務移管
- `Ecology` feedback の範囲
- `Population` / `Settlement` 側の読取先変更

この proposal でまだ決めないこと:

- 詳細な数式
- 各係数・閾値の最終値
- ベンチ設計
- 具体的な UI 可視化仕様
- `reference` への昇格タイミング

## Design Decision

### 1. モジュールは残す

`Subsistence` は Tier 1 module として残す。

理由:

- このコードベースでは `Module` は「近い読み書きを持つ複数 `System` を束ねる単位」である
- 生業戦略更新、食料供給導出、土地利用 pressure 導出は強く関連している
- ここで module を細分化すると、現段階では責務分離の利得より接続コストが大きい

したがって、責務分解は module 分割ではなく module 内部の system 分割で表現する。

### 2. 内部を複数 system に分ける

`Subsistence` は少なくとも次の system で構成する。

1. `AccessSystem`
2. `StrategySystem`
3. `OutputSystem`
4. `PressureSystem`

`PressureSystem` も当面は `Subsistence` の内部 system とする。
ただし将来、人口密度や定住強度への依存が大きくなった場合は、
別 module へ切り出す余地を残す。

### 3. 水アクセスは `Hydrology` の責務に移す

現行の `freshwater_access` は `Subsistence` が書いているが、
これは生業構成の結果ではなく、水系由来の人間利用可能性 proxy である。

したがって、本 proposal では次を採用する。

- `freshwater_access` は廃止する
- `Hydrology` が `surface_water_access` を書く
- `Population` / `Settlement` は `Hydrology.surface_water_access` を読む
- `Subsistence` は `surface_water_access` を読むことはあっても書かない

### 4. 生業表現は 5 軸を維持する

3 軸 `foraging / farming / pastoralism` への単純化は採用しない。

理由:

- `gathering` / `hunting` / `fishing` は access 条件が異なる
- aquatic resource 依存を `foraging` に吸収すると、湖沼・河川依存の社会を潰しやすい
- 牧畜は将来さらに細分化しうるが、現段階では 5 軸の方が拡張余地を保てる

そのため、公開 state としては当面 5 軸 `SubsistenceMix` を維持する。

## Proposed Model

### 公開 state

`Subsistence` は次の state を公開する。

- `subsistence_mix`
- `food_energy`
- `food_stability`

`Hydrology` は次の state を公開する。

- `surface_water_access`

`food_production` は廃止し、`food_energy` と `food_stability` に分ける。

理由:

- `Population` に効くのは平均供給量だけではなく、供給安定性でもある
- 狩猟・採集・漁撈・牧畜は平均収量と安定性の組み合わせが異なる
- `Settlement` にとっても、定住化や都市化は供給量だけでなく安定性の影響を受ける

両指標は v1 では `0.0..=1.0` の正規化 proxy とし、
下流 module が比較可能な相対指標として使えることを契約に含める。

### `SubsistenceMix`

```rust
struct SubsistenceMix {
    gathering:   f32,
    hunting:     f32,
    fishing:     f32,
    cultivation: f32,
    herding:     f32,
}
```

各フィールドは `0.0..=1.0` の連続値で、合計が `1.0` になるよう正規化して保持する。

### 各軸の意味

- `gathering`
  野生植物採集への依存
- `hunting`
  野生動物狩猟への依存
- `fishing`
  河川・湖沼・沿岸を含む水産資源利用への依存
- `cultivation`
  栽培を主とする食料生産への依存
- `herding`
  家畜飼養・放牧を主とする食料生産への依存

この mix は「実施した活動量」ではなく、
各セルがどの獲得戦略にどれだけ依存しているかを表す配分とする。

### 追加の内部派生量

`Subsistence` は内部 system 間で次の派生量を使ってよい。

- `wild_plant_access`
- `wild_animal_access`
- `aquatic_access`
- `arable_potential`
- `grazing_potential`
- 各生業軸の `expected_energy`
- 各生業軸の `expected_stability`

これらは v1 では `Subsistence` 内部の計算用量とし、
公開 state にすることは必須としない。

## Module Responsibilities

### `Subsistence` が決めるもの

- 各セルの生業依存配分 (`subsistence_mix`)
- 各セルの食料供給量 proxy (`food_energy`)
- 各セルの食料供給安定性 proxy (`food_stability`)
- `Ecology` に返す土地利用 pressure

### `Subsistence` が決めないもの

- 表流水アクセス (`surface_water_access`)
- 人口変動
- 集落形成
- 国家形成
- 近傍セルからの生業伝播
- 交易ネットワークによる補完
- polity や文化圏による生業選好

## System Breakdown

### 1. `AccessSystem`

役割:

- 環境条件と家畜化状態から、各戦略に対応する access / potential と
  軸別期待値を導出する

読むもの:

- `Hydrology`
    - `river_flow`
    - `is_lake`
    - `surface_water_access`
- `Ecology`
    - 植生
    - 地被
    - 土壌 fertility
- `Domesticates`
    - `crop_adoption`
    - `livestock_adoption`

書くもの:

- 内部派生量
    - `wild_plant_access`
    - `wild_animal_access`
    - `aquatic_access`
    - `arable_potential`
    - `grazing_potential`
    - 各生業軸の `expected_energy`
    - 各生業軸の `expected_stability`

補足:

- `surface_water_access` 自体は `Hydrology` の責務であり、この system はそれを入力として使う
- `aquatic_access` は `river_flow` / `is_lake` / `surface_water_access` を統合した生業向け proxy とする
- `expected_energy` は各生業軸を主とした場合の期待収量 proxy を表す
- `expected_stability` は各生業軸を主とした場合の期待安定性 proxy を表す
- 期待値は strategy 判断用の内部量であり、公開 state の `food_energy` /
  `food_stability` そのものではない

### 2. `StrategySystem`

役割:

- 前 tick の `subsistence_mix` と access / potential から target mix を計算し、
  慣性付きで次の `subsistence_mix` を決める

読むもの:

- `subsistence_mix` の前 tick 状態
- `AccessSystem` の内部派生量
    - `wild_plant_access`
    - `wild_animal_access`
    - `aquatic_access`
    - `arable_potential`
    - `grazing_potential`
    - 各生業軸の `expected_energy`
    - 各生業軸の `expected_stability`

書くもの:

- `subsistence_mix`

更新則:

- 現在セルの access / potential と軸別期待値から target mix を計算する
- 前 tick の mix から target mix へ緩和する
- 最後に合計 1.0 へ正規化する
- ただし全軸の重みが 0.0 になった場合は、前 tick の mix を維持するか
  均等配分へフォールバックする

判断原理:

- `StrategySystem` は公開 `food_energy` / `food_stability` を読まない
- 各生業軸の `expected_energy` と `expected_stability` を用いて、
  各軸の魅力度を評価する
- 高収量だが不安定な戦略と、低収量だが安定した戦略を区別できるようにする
- v1 では mix 多様化に安定性ボーナスを持たせてよい
- したがって、戦略選択は平均収量のみでなく供給安定性も見て決まる

v1 では含めないもの:

- 近傍セルからの戦略伝播
- 交易による食料補完
- polity や文化圏による嗜好補正

### 3. `OutputSystem`

役割:

- `subsistence_mix` と環境条件から、人口・定住側が読む food 系 state を導出する

読むもの:

- `subsistence_mix`
- `AccessSystem` の内部派生量
    - 各生業軸の `expected_energy`
    - 各生業軸の `expected_stability`
- `surface_water_access`

書くもの:

- `food_energy`
- `food_stability`
- `land_use_intensity`

補足:

- `OutputSystem` は軸別期待値を読んだ上で、
  確定した `subsistence_mix` に応じて公開 state を集計する
- 必要な追加計算は `OutputSystem` 側で行ってよい
- `land_use_intensity` は `PressureSystem` 向けの内部 proxy とし、
  下流 module への公開 state には含めない

#### `food_energy`

- `0.0..=1.0` の正規化 proxy
- 各セルの平均的な食料供給余力を表す
- `Population` の carrying capacity や成長余地計算に使う
- `Settlement` の定住成立や規模拡大の前提条件に使う
- 農耕・牧畜は高収量化しやすいが、環境条件と adoption に強く依存する

#### `food_stability`

- `0.0..=1.0` の正規化 proxy
- 各セルの食料供給の年々・季節間安定性を表す
- `Population` の死亡率ショックや供給変動耐性に使う
- `Settlement` の定住維持や集約化の持続性に使う
- 水産資源・混合戦略・水アクセスは安定性を改善しうる
- mix の多様化は安定性ボーナスを持ちうる
- 単一戦略依存や脆弱な環境では低くなりうる

#### 下流利用契約

- 同一条件下では `food_energy` が高いほど `Population` の人口支持力は下がらない
- 同一条件下では `food_stability` が高いほど `Population` の供給ショック脆弱性は上がらない
- `Settlement` は `food_energy` のみでなく `food_stability` も読む
- v1 では両指標の厳密な合成式や閾値は固定しないが、上記の単調性は守る

### 4. `PressureSystem`

役割:

- `subsistence_mix` をもとに `Ecology` へ返す pressure を導出する

読むもの:

- `subsistence_mix`
- `land_use_intensity`
- 必要に応じて `surface_water_access`

書くもの:

- `logging`
- `grazing`
- `anthropogenic_fire`
- `cultivation_pressure`
- `nutrient_pressure`

v1 では `Population` や `Settlement` を直接読まない。
つまり pressure は生業依存配分に利用強度 proxy を掛けた近似として扱う。

`nutrient_pressure` は `land_use_intensity` と `cultivation` の総合値として扱う。
v1 では農耕による養分収奪や土壌疲弊の proxy とし、厳密な養分循環モデルは持たない。

将来、人口密度や定住強度の影響を入れたくなった場合は、
この system の責務再編を別 proposal で行う。

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
- 自身の前 tick 状態
    - `subsistence_mix`

### 読まないもの

- 気候の生値
- 人口
- 国家
- 近傍セルの生業構成
- 交易量

気候の影響は `Hydrology` / `Ecology` / `Domesticates` の公開値を通じて受ける。

## Downstream Changes

### `Hydrology`

新たに次を書く。

- `surface_water_access`

定義:

- 人間が地表水に到達し利用できる程度の proxy
- 飲用、生活用水、基礎的生業に使える表流水アクセスを表す
- 灌漑能力や地下水アクセスの完全代理ではない

### `Population`

`Population` は次を読む。

- `food_energy` ← `Subsistence` が書く
- `food_stability` ← `Subsistence` が書く
- `surface_water_access` ← `Hydrology` が書く

`Population` は単一の `food_production` に依存しない形へ変更する。

利用契約:

- `food_energy` を人口支持力と成長余地の主入力として使う
- `food_stability` を死亡率ショックや供給変動耐性の主入力として使う
- v1 では両指標の具体的な合成式は固定しないが、両方を読む前提を固定する

### `Settlement`

`Settlement` は次を読む。

- `subsistence_mix` ← `Subsistence` が書く
- `food_energy` ← `Subsistence` が書く
- `food_stability` ← `Subsistence` が書く
- `surface_water_access` ← `Hydrology` が書く

利用契約:

- `food_energy` を定住成立や規模拡大の主入力として使う
- `food_stability` を定住維持や集約化持続性の主入力として使う
- v1 では具体的な閾値や重みは固定しないが、`food_stability` を独立入力として読む前提を固定する

## Ecology Feedback

`Subsistence` は `Ecology` に対して次の pressure を送る。

- `logging`
- `grazing`
- `anthropogenic_fire`
- `cultivation_pressure`
- `nutrient_pressure`

### 生業ごとの主な圧力

- `gathering`
    - 基本は低圧
    - 条件に応じて弱い `anthropogenic_fire` や `logging` を持ちうる
- `hunting`
    - 植生圧は低い
    - 条件に応じて弱い `anthropogenic_fire` を持ちうる
- `fishing`
    - 陸上植生圧は低い
    - v1 では `Ecology` への直接圧は小さいものとして扱う
- `cultivation`
    - 主に `cultivation_pressure`
    - 条件に応じて `anthropogenic_fire`
    - 条件に応じて弱い `logging`
- `herding`
    - 主に `grazing`

`slash_burn` は農耕専用の名称としては使わず、
`anthropogenic_fire` の一部として扱う。

## Compatibility Notes

本 proposal は、現行の `reference` と暫定実装の前提を変更する。

主な変更点:

- `food_production` を廃止し、`food_energy` / `food_stability` に置き換える
- `freshwater_access` を廃止し、`Hydrology.surface_water_access` に置き換える
- `farming` / `pastoralism` は `cultivation` / `herding` に改名する
- `Subsistence` は内部的に複数 system を持つ前提へ変わる

したがって、採用時には少なくとも次の更新が必要になる。

- `docs/reference/architecture/data_model.md`
- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/modules/hydrology.md`
- `docs/reference/modules/ecology.md`
- `Population` / `Settlement` / `Subsistence` の実装

## Acceptance Criteria

この proposal が固まった状態とは、implementer が次を迷わず答えられる状態を指す。

- `Subsistence` が何を読むか
- `Subsistence` が何を書くか
- `Hydrology` へ移す state は何か
- 内部 system は何に分かれるか
- `SubsistenceMix` の 5 軸は何を意味するか
- `food_energy` と `food_stability` の役割差は何か
- `Population` / `Settlement` が両指標をどういう役割で読むか
- `Ecology` への feedback 範囲は何か
- 近傍影響と交易補完を v1 で含めないこと

## Open Items

次段階で別途決める。

- `surface_water_access` の具体的スケーリング
- `food_energy` と `food_stability` の具体的変換式
- `Population` が両指標をどう合成するかの数式
- `Settlement` が両指標に与える重みと閾値
- `PressureSystem` に人口密度・定住強度をいつ導入するか
- `reference` 昇格時の移行手順
