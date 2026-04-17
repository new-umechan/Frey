# Subsistence Module Proposal

## Status

Draft

## Goal

`Subsistence` モジュールの責務、入力、出力、および他モジュールとの接続点を、実装に移れる粒度で定義する。

この proposal は、現行の暫定 5 軸 `SubsistenceMix` を置き換える候補として、
3 軸 mix 案を固定するための文書である。

## Scope

この proposal で決めること:

- `Subsistence` が何を読むか
- `Subsistence` が何を書くか
- 生業 mix の軸と意味
- mix の更新則
- `food_production` と `freshwater_access` の役割
- `Domesticates` / `Ecology` / `Hydrology` / `Population` / `Settlement` との接続
- `Subsistence -> Ecology` feedback の範囲と意味

この proposal でまだ決めないこと:

- 詳細な数式
- 各係数・閾値の最終値
- ベンチ設計
- `reference` への昇格タイミング

## Current Constraints

- `docs/reference/architecture/module_boundaries.md` では、`Subsistence` は
  `subsistence_mix`, `food_production`, `freshwater_access` を書く前提になっている
- `docs/reference/architecture/data_model.md` にはこれらの列がすでに存在する
- `Domesticates` は `crop_adoption` / `livestock_adoption` を公開し、
  `Subsistence` は `adoption` のみを読む前提になっている
- 現行の `SubsistenceMix` は 5 軸
  (`gathering`, `hunting`, `fishing`, `farming`, `pastoralism`) だが、
  本 proposal では 3 軸への単純化を置換候補として採用する

## Proposed Model

### 1. 生業 mix の定義

`SubsistenceMix` は次の 3 軸割合で表現する。

- `foraging`
- `farming`
- `pastoralism`

各フィールドは `0.0..=1.0` の連続値で、合計が `1.0` になるよう正規化して保持する。

```rust
struct SubsistenceMix {
    foraging:    f32,
    farming:     f32,
    pastoralism: f32,
}
```

### 2. 各軸の意味

- `foraging`
  採集・狩猟・漁撈を含む生活様式
- `farming`
  栽培を主とする生活様式
- `pastoralism`
  牧畜を主とする生活様式

`foraging` は「農耕でも牧畜でもない残差」ではなく、
採集・狩猟・漁撈を含む独立した生活様式として扱う。

### 3. v1 の責務境界

`Subsistence` は各セルのローカル条件と前 tick の自身の状態だけを使って更新する。

v1 では含めないもの:

- 近傍セルからの生業伝播
- 交易ネットワークによる生業変化
- 文化圏や polity の影響

これらは将来の `Settlement` や上位モジュールの責務とする。

## Inputs

`Subsistence` は次を読む。

- `Hydrology`
  - `river_flow`
  - `is_lake`
- `Ecology`
  - 植生・地被・土壌に相当する公開値
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

気候の影響は、`Ecology` / `Hydrology` / `Domesticates` の公開値を通じて受ける。

## Outputs

`Subsistence` は次を書く。

- `subsistence_mix`
- `food_production`
- `freshwater_access`

### `food_production`

Population / Settlement に渡す単一スカラーの総食料産出 proxy とする。

- 生業別の内訳は v1 では公開しない
- 内部では 3 軸 mix と環境適性・adoption 補正から導出する
- `Subsistence` は供給側に徹し、人口支持力や人口応答そのものは書かない

### `freshwater_access`

`river_flow` と `is_lake` から導出する独立公開指標とする。

- 生業 mix の一部ではない
- 飲用・生活・基礎的生業に使える淡水アクセスの proxy として扱う
- Population / Settlement が直接読んでよい
- v1 では公共資源指標として扱う
- 灌漑ポテンシャルそのものまでは含めない

## Update Rule

### 1. target mix の計算

各 tick で、現在セルの環境条件から target mix を計算する。

入力に使う主な値:

- `Ecology`
  - 植生・地被・土壌
- `Hydrology`
  - `river_flow`, `is_lake`
- `Domesticates`
  - `crop_adoption`, `livestock_adoption`

方向性:

- `crop_adoption` が高いほど `farming` の圧力を上げる
- `livestock_adoption` が高いほど `pastoralism` の圧力を上げる
- 植生・水系条件が強いほど `foraging` を支える

### 2. 前 tick からの緩和

mix は毎 tick で即時に切り替えず、前 tick の `subsistence_mix` から
target mix へ徐々に近づける。

つまり v1 の更新則は次で固定する。

- target mix を計算する
- 前 tick の mix から target mix へ緩和する
- 最後に合計 1.0 へ正規化する

これにより、生業転換には慣性があることを表現する。

## food_production の考え方

`food_production` は、3 軸 mix それぞれに対応する生産ポテンシャルを計算し、
その重み和として求める。

概念上の方向性:

- `foraging`
  - 植生・水系に強く依存する
  - `Domesticates` への依存は弱い、または持たない
- `farming`
  - `crop_adoption` に強く依存する
  - 土壌と水系にも依存する
- `pastoralism`
  - `livestock_adoption` に強く依存する
  - 地被と水系にも依存する

Population は `food_production` を読んで人口変動を計算するが、
`Subsistence` は人口モデル自体を持ち込まない。

## Ecology Feedback

`Subsistence` は `Ecology` に対して広めの feedback を送る。

v1 で含めるもの:

- `logging`
- `grazing`
- `slash_burn`
- `farming_consumption`
- soil 側の `slash_burn_delta`

意味づけ:

- `logging`
  伐採強度
- `grazing`
  放牧による地被消耗
- `slash_burn`
  焼畑による tree / ground cover への影響
- `farming_consumption`
  農耕・連作による soil fertility 消耗
- `slash_burn_delta`
  焼畑による soil fertility への増減

### 生業ごとの主な圧力

- `foraging`
  - 基本は低圧
  - 条件に応じて弱い `logging` を持ちうる
- `farming`
  - 主に `farming_consumption`
  - 必要に応じて `slash_burn`
  - 条件に応じて弱い `logging`
- `pastoralism`
  - 主に `grazing`

`slash_burn` は v1 では農耕側の pressure として扱う。

## Module Responsibilities

### `Subsistence` が決めるもの

- 各セルの生業 mix
- 総食料産出 proxy
- 淡水アクセス
- `Ecology` に返す土地利用 pressure

### `Subsistence` が決めないもの

- 人口変動
- 集落形成
- 国家形成
- 近傍からの伝播
- 文化圏による生業選好

## Compatibility Notes

現行の `reference` と暫定実装では 5 軸 `SubsistenceMix` が前提になっている。

本 proposal の位置づけは次の通り。

- 現行 5 軸は暫定実装
- 本 proposal の 3 軸は置換候補
- まだ `reference` の正本は更新しない

したがって、次段階で本 proposal を採用する場合は、
`reference` と実装の両方で 5 軸から 3 軸への移行が必要になる。

## Acceptance Criteria

この proposal が固まった状態とは、implementer が次を迷わず答えられる状態を指す。

- `Subsistence` が何を読むか
- 何を書くか
- 3 軸の意味
- `food_production` と `freshwater_access` の役割差
- `Ecology` への feedback 範囲
- 生業ごとの主な pressure
- 近傍影響を v1 で含めないこと

## Open Items

次段階で別途決める。

- mix 導出式の詳細
- 緩和係数
- `freshwater_access` の具体的スケーリング
- `food_production` の正規化単位
- `reference` 昇格時の移行手順
