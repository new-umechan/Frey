# Domesticatesの詳細仕様

## 目的

Domesticatesは、各セルでどの作物・家畜が環境的に成立しうるかと、
どの程度まで実際に普及しているかを近似計算する。

毎tickで次の値を `World State` に書く。

- 作物栽培可能種（`crop_available`）: Domesticates内部専用。次tickのadoption更新で自身のみが参照する
- 作物普及度（`crop_adoption`）: 0.0〜1.0。Subsistenceが読む
- 家畜利用可能種（`livestock_available`）: Domesticates内部専用。次tickのadoption更新で自身のみが参照する
- 家畜普及度（`livestock_adoption`）: 0.0〜1.0。Subsistenceが読む

このモジュールは収量そのものを計算しない。
`Subsistence` が読むのは `adoption` のみであり、
`Domesticates` は「何が使えるか」「どこまで広がっているか」を担当する。

## 入力

Domesticatesが読む主な値は次のとおり。

- `climate.temperature`
- `climate.precipitation`
- `climate.aridity`
- `geology.height`
- `hydrology.river_flow`
- `ecology.biome`
- `ecology.tree_cover`
- `ecology.ground_cover`
- `ecology.soil_fertility`
- `clock.epoch`
- 前tickの `crop_adoption`
- 前tickの `livestock_adoption`
- FeedbackQueue（`Settlement` から届く拡散圧）
- FeedbackQueue（`Population` から届く人口密度圧）

`clock.epoch` は、先史期以降に Domesticates を有効化するために読む。
初期成立中心シードはモジュール初回有効tickで初期化する。

FeedbackQueue で受けるのは、近傍セルの自然拡散ではなく、次の2種類の非局所圧だけである。

- `Settlement` からの拡散圧：移住・交易接触・定住ネットワーク経由の持ち込み
- `Population` からの人口密度圧：集約化インセンティブのスケーリング係数として使う

## 出力

Domesticatesは次の配列を全セル分持つ。

```rust
// u8で8種の作物をビット管理
// bit0: Wheat, bit1: Rice, bit2: Maize,    bit3: Millet
// bit4: Potato, bit5: Cassava, bit6: Sorghum, bit7: Yam
type CropBitmap = u8;

// u8で5種の家畜をビット管理
// bit0: Cattle, bit1: Horse, bit2: Sheep, bit3: Pig, bit4: Camel
type LivestockBitmap = u8;

const N_CROPS: usize = 8;
const N_LIVESTOCK: usize = 5;
```

- `crop_available: Vec<CropBitmap>`
- `crop_adoption: Vec<[f32; N_CROPS]>`
- `livestock_available: Vec<LivestockBitmap>`
- `livestock_adoption: Vec<[f32; N_LIVESTOCK]>`

`available` と `adoption` の意味は明確に分ける。

- `available`
  現在の環境条件だけを見た「成立可能フラグ」
- `adoption`
  そのセルで実際に利用・栽培・飼養されている程度（`0.0..=1.0`）

`Subsistence` は `available` を読まず、`adoption` のみを読む。

## データ型の補足

### カテゴリ粒度

このモジュールのカテゴリは、厳密な単一種ではなく
「下流の生業差分に必要な代表的アーキタイプ」として扱う。

- `Wheat`
  温帯寄りのコムギ類。冷涼〜温暖の中庸環境を基準にする基幹穀物
- `Rice`
  高温多水・低地水利用寄りのイネ類。湿田稲作を主対象とする
- `Maize`
  暖温帯から熱帯寄りのトウモロコシ類。高温で中〜高水分域に強い
- `Millet`
  半乾燥・短期栽培寄りの雑穀類。アフリカ・内陸アジアの乾燥適応を代表する
- `Potato`
  冷涼高地寄りの塊茎作物。高原・山麓の低温環境に強い
- `Cassava`
  高温乾燥耐性の塊茎作物。熱帯低地の半乾燥〜中水分域を代表する
- `Sorghum`
  高温かつ最も強い乾燥耐性を持つ穀物。アフリカ半乾燥帯の基幹作物
- `Yam`
  高温多湿・森林縁寄りの塊茎作物。熱帯湿潤域の補完的食料源

- `Cattle`
  開放地・草地寄りの大型反芻家畜
- `Sheep`
  乾燥・半乾燥草地寄りの小型反芻家畜
- `Pig`
  湿潤・森林縁・農耕近接寄りの家畜
- `Horse`
  開放草地寄りで長距離移動に向く家畜
- `Camel`
  高温乾燥地寄りの家畜

学術的には粗いカテゴリであるため、次を明記する。

- `Millet` はトウジンビエ・シコクビエ・キビ・アワなどを束ねる
- `Yam` はDioscorea属各種を束ねる。`Cassava` とは起源・ニッチが異なるため分離する
- v1 では分類学的厳密性よりも、生業分化に必要な環境ニッチ差を優先する
- `Subsistence` は上位2〜3作物の `adoption` バランスで混合農耕を判定する。カテゴリ数による混合判定は行わない

## 処理ロジック

Domesticatesは、各作物・家畜について次の順で更新する。

1. 環境条件から `niche_score` を計算する
2. `niche_score` から `available` を判定する
3. 初回有効tickで初期成立中心シードを作る
4. 前tickの普及度と近傍拡散圧から `adoption` を更新する

### 共通ヘルパー

```rust
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn converge_toward(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * rate * dt
}

fn gaussian_score(x: f32, mean: f32, sigma: f32) -> f32 {
    let z = (x - mean) / sigma.max(1e-6);
    (-0.5 * z * z).exp()
}
```

### 内部評価量

各カテゴリについて、少なくとも次の連続量を内部で計算する。
これらは公開列にはしない。

- `temperature_score`
- `moisture_score`
- `terrain_score`
- `cover_score`
- `fertility_or_pasture_score`
- `river_bonus`
- `niche_score`
- `terrain_conductance`
- `spread_pressure`
- `intensification_factor`

### available 判定

`available` は連続スコアから閾値判定する。
単純なif分岐の列ではなく、複数環境要因の積または重み付き最小値で近似する。

概念式:

```text
niche_score =
    temperature_score
  * moisture_score
  * terrain_score
  * cover_score
  * fertility_or_pasture_score

available = niche_score >= species_threshold
            and not hard_exclusion
```

`river_bonus` は主に `Rice`、`Yam`、`Pig` の適地補正に使う。
`hard_exclusion` は少数に限定する。

例:

- `Rice` は極端な乾燥セルで除外
- `Camel` は湿潤森林優勢セルで除外
- `Horse` は密林・高山で強く不利
- `Pig` は極端な乾燥開放地で強く不利

### 初期成立中心シード

現実世界の固定座標や歴史的原産地は使わない。
生成世界内で、そのカテゴリが比較的早く成立しやすい
「初期成立中心」を抽出する。

ここで作るのは、文献上の原産地そのものではない。
環境適地に加えて、人間が継続的に接触・管理・持ち込みを行いやすい条件が
重なる場所に、初期の成立シードを置く。

初回有効tickで、各カテゴリごとに次を実行する。

1. 全セルで `niche_score` を計算する
2. 全セルで `corridor_score` と `human_management_score` を計算する
3. `origin_potential = niche_score * corridor_score * human_management_score` を計算する
4. `origin_potential` 上位帯のセルを候補にする
5. 候補セルを連結領域へまとめる
6. 面積が小さすぎる領域を除外する
7. 残った領域から最大 `origin_count_limit` 個まで中心を選ぶ
8. 選ばれたセル群に初期 `adoption` を小さく与える

`corridor_score` は、河川・低地回廊・沿岸・移動容易性など、
成立と初期拡散の両方を助ける地理条件をまとめた連続量である。
v1 では少なくとも `hydrology.river_flow` と `geology.height` から導く。

`human_management_score` は、人間がそのカテゴリを継続的に試行・管理しやすい
条件の近似量である。
v1 では独立の人口モジュールに依存せず、少なくとも次の proxy から作る。

- 河川近接または低地であること
- 極端環境ではないこと
- 植生被覆が完全閉鎖でも完全裸地でもないこと

`Population` からの人口密度feedbackは adoption 側の集約化圧にのみ使う。
起源シード抽出の `human_management_score` には直接入れず、
初期成立条件と後期の人口集積効果を分離する。

方針:

- 1カテゴリ1起源とは限らず、妥当な成立中心が複数あれば複数シードを許す
- ただしシード乱立は避けるため、上限個数を持つ
- 同一seed・同一worldなら決定的に同じ成立中心が選ばれるようにする
- 高適地でも、人間活動条件が弱い場所は初期成立中心になりにくくする
- 逆に接触条件が良くても、環境不適地だけで成立中心にはしない

### adoption 更新

`adoption` は、環境適合度・拡散圧・集約化インセンティブの3要素で決まる。

概念式:

```text
intensification_factor =
    1.0 + population_pressure_bonus
    // population_pressure_bonus は Population からの FeedbackQueue で上書きされる
    // Population が未接続の tick では 0.0 固定（factor = 1.0）

target_adoption =
    clamp01(
        available_gate
      * max(origin_seed_strength, spread_pressure)
      * intensification_factor
    )
    // intensification_factor が 1.0 を超えても、target_adoption は 1.0 を上限とする
    // clamp01 はここでのみ適用し、next_adoption への収束式には影響しない

next_adoption =
    current
  + growth_rate * (target_adoption - current) * dt
  - decay_rate * unsuitability_penalty * dt
```

更新則の原則は次のとおり。

- `available = false` なら急にゼロにせず、徐々に減衰する
- `available = true` でも、拡散圧がなければ adoption は急上昇しない
- 起源地では初期 `adoption` を持つため先に立ち上がる
- 近傍からの拡散で周辺セルが遅れて追随する
- 1tickで `0.0 -> 1.0` に飛ばさない
- 人口密度が高いセルでは `intensification_factor` が上昇し、adoption の立ち上がりが加速する

ここでの慣性は `current` 自体が担う。
別の内部状態として `retained_tradition` は持たない。
「過去に普及していたため急には消えない」という性質は、
`current` から `target_adoption` への収束と、
不適地での緩い `decay_rate` により表現する。

### spread_pressure

普及圧は、地形摩擦を考慮した近傍伝播と、非局所feedbackの合算とする。

```text
terrain_conductance =
    f(hydrology.river_flow, geology.height, ecology.biome)
    // 河川・低地回廊で高く、山脈・砂漠・密林で低い
    // corridor_score と同一の地理変数から導く

local_spread =
    local_neighbor_adoption
  * terrain_conductance

spread_pressure =
    local_spread
  + routed_feedback_bonus
```

- `local_neighbor_adoption`
  近傍セルの `adoption` から計算する内生拡散圧
- `terrain_conductance`
  地形・水文による拡散速度の修飾。同一採用量でも、地形条件で広がりやすさが変わる
- `routed_feedback_bonus`
  `Settlement` が前tickに積む追加圧

feedback の責務は次で固定する。

- 近傍セルからの通常拡散は `Domesticates` 自身が現在tickの近傍 `adoption` を読んで計算する
- `Domesticates` 自身は自分の inbox に feedback を積まない
- `routed_feedback_bonus` は `Settlement` が、移住・交易接触・定住ネットワーク経由の持ち込みを表すときだけ積む
- `population_pressure_bonus` は `Population` が、人口密度由来の集約化圧として積む
- これらの feedback は `FeedbackEntry.target_module = ModuleId::Domesticates` で配送する

概念例:

```rust
// Settlement からの作物拡散圧
FeedbackEntry {
    target_module: ModuleId::Domesticates,
    payload: FeedbackPayload::DeltaF32 {
        field: CellFieldId::DomesticatesRoutedCropFeedback(crop_id),
        cell: target_cell,
        delta: crop_delta,
    },
}

// Settlement からの家畜拡散圧
FeedbackEntry {
    target_module: ModuleId::Domesticates,
    payload: FeedbackPayload::DeltaF32 {
        field: CellFieldId::DomesticatesRoutedLivestockFeedback(livestock_id),
        cell: target_cell,
        delta: livestock_delta,
    },
}

// Population からの人口密度圧
FeedbackEntry {
    target_module: ModuleId::Domesticates,
    payload: FeedbackPayload::DeltaF32 {
        field: CellFieldId::DomesticatesIntensificationBonus,
        cell: target_cell,
        delta: intensification_bonus,    // population_pressure_bonus に加算する
    },
}
```

これにより、先史期の初期段階でも `Settlement` や `Population` の成熟を前提にせず、
Domesticates単体で起源地からの緩い拡散を表現できる。
`Population` が接続されたあとは、人口密度の高い地域で採用強化が加速する挙動が自然に生まれる。

## 種ごとのニッチ方針

具体的なパラメータ値は実装時に調整するが、
仕様として必要な環境方向性は次で固定する。

### 評価軸の使い分け

各カテゴリは共通の内部評価量を使うが、どの軸を強く効かせるかは異なる。
実装時は一律の重みではなく、カテゴリごとに強弱を分ける。

- 温度依存が強いカテゴリ
  `Rice`、`Maize`、`Sorghum`、`Cassava`、`Yam`、`Camel`
- 水分依存が強いカテゴリ
  `Rice`、`Yam`、`Pig`
- 乾燥耐性が強いカテゴリ
  `Sorghum`、`Cassava`、`Millet`、`Camel`、`Sheep`
- 地形依存が強いカテゴリ
  `Potato`（高地）、`Horse`（平坦開放地）
- 植生被覆依存が強いカテゴリ
  `Pig`、`Yam`（森林縁）、`Horse`、`Camel`（開放地）
- 河川・低地補正が強いカテゴリ
  `Rice`、`Yam`、`Pig`
- `terrain_conductance` をカテゴリ別にパラメータ化する
  同一の `terrain_conductance_weights`（river_flow_w・height_w・biome_w）を全カテゴリ共通にせず、
  カテゴリごとに独立した重みを持つ。拡散ネットワークの種別差を表現するための主要手段とする

### 作物

#### `Wheat`

温帯の中庸環境を基準にする基幹穀物カテゴリ。

- `temperature_score`
  中温域で最大。高温多湿より、冷涼から温暖の安定域を高く評価する
- `moisture_score`
  極端な乾燥と過湿を避ける。中程度降水で高い
- `terrain_score`
  極端な高地で減点するが、低地専用ではない
- `cover_score`
  密林より、疎林から開放地で高い
- `fertility_or_pasture_score`
  `ecology.soil_fertility` を比較的強く効かせる
- `river_bonus`
  あってよいが主役ではない。氾濫原での成立補助程度にとどめる
- `hard_exclusion`
  極端な寒冷、極端な湿潤湛水、極端な乾燥で除外候補
- `origin_potential`
  中低地の回廊と開けた肥沃地を優先する

#### `Rice`

高温多水かつ低地水利用を強く必要とする水分依存カテゴリ。湿田稲作を主対象とする。

- `temperature_score`
  高温域で最大。低温側は急減させる
- `moisture_score`
  高降水で高い。乾燥側では急減する
- `terrain_score`
  低地・平坦地を優遇する。急峻地形は不利
- `cover_score`
  密林そのものではなく、低地湿潤で管理可能な開放域または森林縁を高くみる
- `fertility_or_pasture_score`
  `soil_fertility` も見るが、水条件の優先度を上回らない
- `river_bonus`
  最も強く効かせる。`hydrology.river_flow` と低標高の組み合わせで大きく上がる
- `hard_exclusion`
  極端な乾燥セルは除外する
- `origin_potential`
  河川下流、デルタ、氾濫原、湖沼周辺低地を優先する
- `terrain_conductance_weights`
  river_flow_w を最大にする。山地越えで伝導度を大きく落とす

#### `Maize`

暖温帯から熱帯に寄る高温要求作物として扱う。

- `temperature_score`
  高温域で最大。低温に強い罰則を入れる
- `moisture_score`
  中程度からやや高めの水分で高い。`Rice` ほど多水専用ではなく、`Sorghum` より水分要求が高い
- `terrain_score`
  極端な高地や寒冷高原では不利。低地〜温暖高原縁まで許容する
- `cover_score`
  開放地から森林縁で高い
- `fertility_or_pasture_score`
  `soil_fertility` の影響を比較的受ける
- `river_bonus`
  中程度。水利があれば伸びるが必須ではない
- `hard_exclusion`
  低温を主要除外条件にする
- `origin_potential`
  温暖低地から温暖高原縁まで候補を持てる
- `terrain_conductance_weights`
  biome_w を中程度。熱帯〜暖温帯バイオーム内で広がる

#### `Millet`

短期栽培・半乾燥適応の雑穀カテゴリ。

- `temperature_score`
  中温からやや高温で安定し、低温では減衰する
- `moisture_score`
  中低水分域で高く、過湿で下がる。`Sorghum` より水分要求がやや高い
- `terrain_score`
  平地偏重ではなく、やや粗い地形でも成立しうる
- `cover_score`
  草地・疎林・耕作開放地で高い
- `fertility_or_pasture_score`
  低肥沃耐性を比較的広く取る
- `river_bonus`
  小さい。河川補正がなくても成立しうる
- `hard_exclusion`
  恒常的な過湿低地を不利にする
- `origin_potential`
  半乾燥回廊、内陸盆地、草地縁で候補が立ちやすい
- `terrain_conductance_weights`
  height_w を低め。やや粗い地形でも伝わる

#### `Potato`

冷涼高地に適応した塊茎作物カテゴリ。

- `temperature_score`
  冷涼〜温帯で最大。高温で急減する。`Wheat` より低温側に最適域を持つ
- `moisture_score`
  中程度。過湿・過乾燥を避けるが `Rice` ほど水分依存は強くない
- `terrain_score`
  高地・丘陵を許容する。高地減点を `Wheat` より弱める
- `cover_score`
  開放地から疎林で高い
- `fertility_or_pasture_score`
  中程度。高地痩地でも即不成立にはしない
- `river_bonus`
  小さい
- `hard_exclusion`
  高温低地を主要除外条件にする
- `origin_potential`
  高原縁、山麓、冷涼中緯度帯で候補が立つ
- `terrain_conductance_weights`
  height_w を低め。高地でも伝導度を落としすぎない

#### `Cassava`

高温乾燥耐性の塊茎作物カテゴリ。熱帯低地の半乾燥〜中水分域を代表する。

- `temperature_score`
  高温域で最大。低温で急減する
- `moisture_score`
  半乾燥〜中水分域で高い。`Sorghum` より水分要求がやや高いが乾燥耐性は `Maize` より強い
- `terrain_score`
  低地から中程度の斜面まで。極端な高地は不利
- `cover_score`
  開放地から森林縁まで比較的広く受ける
- `fertility_or_pasture_score`
  痩地耐性が比較的強い。低肥沃でも成立しうる
- `river_bonus`
  小さい
- `hard_exclusion`
  低温を主要除外条件にする
- `origin_potential`
  熱帯低地、サバンナ縁、半乾燥森林縁で候補が立つ
- `terrain_conductance_weights`
  biome_w を中程度。熱帯バイオーム内で広がる

#### `Sorghum`

高温かつ最も強い乾燥耐性を持つ穀物カテゴリ。アフリカ半乾燥帯の基幹作物。

- `temperature_score`
  高温域で最大。低温で急減する
- `moisture_score`
  低水分域で最も高い。3作物（`Maize`・`Cassava`・`Sorghum`）の中で最も乾燥側に最適域を置く
- `terrain_score`
  平地から緩斜面で高い。急峻地形は不利
- `cover_score`
  開放地・サバンナ・疎林で高い
- `fertility_or_pasture_score`
  痩地耐性が強い
- `river_bonus`
  局所水場として弱く効かせる。湿潤低地補正にはしない
- `hard_exclusion`
  低温と恒常的な過湿低地を除外する
- `origin_potential`
  半乾燥内陸、サバンナ帯、乾燥草地縁で候補が立つ
- `terrain_conductance_weights`
  biome_w を高め。乾燥バイオーム内で維持しやすく、湿潤帯に入ると減衰する

#### `Yam`

高温多湿・森林縁寄りの塊茎作物カテゴリ。熱帯湿潤域の補完的食料源。

- `temperature_score`
  高温域で最大。低温で急減する
- `moisture_score`
  高めの水分で有利。`Rice` に次ぐ水分依存度を持つ
- `terrain_score`
  低地から中程度の斜面。急峻地形は不利
- `cover_score`
  中〜高 `tree_cover` の森林縁で高い。完全閉鎖林は下げる
- `fertility_or_pasture_score`
  中程度
- `river_bonus`
  中程度。湿潤低地・河谷で補正する
- `hard_exclusion`
  極端乾燥と低温を除外する
- `origin_potential`
  熱帯湿潤低地、河谷森林縁、混合植生帯で高い
- `terrain_conductance_weights`
  river_flow_w を中程度、biome_w を高め。湿潤熱帯バイオーム内で広がり、乾燥帯への伝導は遅い

### 家畜

#### `Cattle`

大型反芻家畜の代表カテゴリで、放牧可能な開放地依存が強い。

- `temperature_score`
  中温から高温で安定し、極寒は不利
- `moisture_score`
  中庸域で高い。極端乾燥では下がるが `Pig` ほど敏感ではない
- `terrain_score`
  急峻地で減点する
- `cover_score`
  `ground_cover` が高く、`tree_cover` が低いほど高い
- `fertility_or_pasture_score`
  `ground_cover` を主要 proxy とし、草資源量を近似する
- `river_bonus`
  水飲み場・移動回廊として弱く効かせてよい
- `hard_exclusion`
  密林と極高山を不利にする
- `origin_potential`
  草地、サバンナ、河谷沿い開放地で候補が立つ

#### `Sheep`

粗放草地・半乾燥域に強い小型反芻家畜カテゴリ。

- `temperature_score`
  冷涼から温暖まで広め
- `moisture_score`
  乾燥から半乾燥で高く、過湿で下げる
- `terrain_score`
  丘陵・高原を比較的許容する
- `cover_score`
  開放草地で高い。密林では低い
- `fertility_or_pasture_score`
  低い `ground_cover` でも一定成立を残す
- `river_bonus`
  小さい
- `hard_exclusion`
  常時湿潤で閉鎖林優勢のセルを不利にする
- `origin_potential`
  内陸草地、乾燥高原、山麓草原で候補が立ちやすい

#### `Pig`

湿潤・森林縁・定住近接で成立しやすい家畜カテゴリ。

- `temperature_score`
  中温から高温で高い
- `moisture_score`
  高めの水分で有利
- `terrain_score`
  極端な山地は不利だが、低地偏重にしすぎない
- `cover_score`
  中から高 `tree_cover`、ただし完全閉鎖林はやや下げる
- `fertility_or_pasture_score`
  草資源より、人為管理しやすさ proxy を重視する
- `river_bonus`
  中程度。湿潤低地・河谷で補正する
- `hard_exclusion`
  極端乾燥かつ開放地優勢のセルを強く不利にする
- `origin_potential`
  河谷森林縁、湿潤低地、混合植生帯で高い
- `spread_pressure`
  完全開放乾燥帯をまたぐ拡散は遅く、農耕近接帯で強まる

#### `Horse`

開放草地と長距離移動性に寄る家畜カテゴリ。

- `temperature_score`
  冷涼から温暖まで広い
- `moisture_score`
  中低水分域で高い
- `terrain_score`
  平坦から緩斜面で高く、急峻高山で下げる
- `cover_score`
  低 `tree_cover`・高 `ground_cover` を強く好む
- `fertility_or_pasture_score`
  `ground_cover` と移動容易性を合わせて評価する
- `river_bonus`
  河川そのものより低地回廊効果として効かせる
- `hard_exclusion`
  密林と高山で強く不利
- `origin_potential`
  広い草原帯、内陸低地回廊、乾燥ステップで高い
- `spread_pressure`
  回廊地形で高く、森林帯では伝導度を大きく落とす

#### `Camel`

高温乾燥・疎植生帯に特化した家畜カテゴリ。

- `temperature_score`
  高温域で最大
- `moisture_score`
  低水分域で最大。湿潤化で急減する
- `terrain_score`
  平坦から緩い乾燥地形で高い
- `cover_score`
  低 `tree_cover`・低から中 `ground_cover` で高い
- `fertility_or_pasture_score`
  草資源要求は低めに置く
- `river_bonus`
  局所水場としては有効だが、湿潤低地補正にはしない
- `hard_exclusion`
  湿潤森林優勢セルは除外する
- `origin_potential`
  乾燥低地回廊、砂漠縁、疎植生ステップで候補が立つ
- `spread_pressure`
  乾燥回廊では維持しやすく、湿潤森林帯へ入ると減衰しやすい

### 実装上の最低要件

上記の種類別差分は、少なくとも次の3層に反映されていなければならない。

- `niche_score` の最適域と重み
- `hard_exclusion` の有無と強さ
- `origin_potential` / `terrain_conductance` のカテゴリ差

つまり、種別差を `species_threshold` のみで表現してはならない。
少なくとも温度・水分・被覆・回廊補正のうち2軸以上で、
カテゴリごとの非対称性が実装に現れる必要がある。

## パラメータ管理

Domesticates のパラメータは、カテゴリ別にまとめて管理する。

最低限必要なのは次の種類である。

- `species_threshold`
- `temperature_optimum`
- `temperature_sigma`
- `moisture_optimum`
- `moisture_sigma`
- `height_limit`
- `tree_cover_preference`
- `ground_cover_preference`
- `river_bonus_weight`
- `terrain_conductance_weights`（カテゴリごとに独立。river_flow_w・height_w・biome_wの3値。全カテゴリ共通にしない）
- `origin_count_limit`
- `origin_seed_strength`
- `growth_rate`
- `decay_rate`

値そのものはコードに埋め込んでもよいが、
種ごとの閾値・最適値・更新率は一箇所に集約し、散在させない。

## テスト観点

最低限、次のシナリオを満たすこと。

1. 高温多水低地で `Rice.available` が高く、乾燥高地より有利になる
2. 半乾燥冷涼セルで `Millet` と `Sorghum` が `Rice` より有利になる
3. 冷涼高地で `Potato` が `Maize` / `Cassava` / `Yam` より有利になる
4. 高温乾燥開放地で `Sorghum` の `niche_score` が `Cassava` より高く、`Maize` より大幅に高くなる
5. 熱帯湿潤森林縁で `Yam` の `niche_score` が `Sorghum` より高くなる
6. 乾燥開放地で `Camel` / `Sheep` が高く、`Pig` が低くなる
7. 起源地シードが全カテゴリで0件にならない
8. 低適地セルが起源地に選ばれない
9. 適地でも孤立セルは `adoption` が即座に上がらない
10. 不適地へ入ると `adoption` は即座にゼロにならず、遅れて減衰する
11. `Subsistence` は引き続き `crop_adoption` / `livestock_adoption` のみを読める
12. 山脈・砂漠を挟んだセルへの拡散が、低地回廊経由より遅くなる（terrain_conductance）
13. `Rice` の拡散が `Millet` より山地越えで遅く、河川沿いで速い（terrain_conductance カテゴリ差）
14. `Horse` の拡散が森林帯で大きく落ち、ステップ回廊で速い（terrain_conductance カテゴリ差）
15. `Population` feedback未接続時、`intensification_factor = 1.0` で動作が変わらない
16. `Population` feedback接続後、人口密度の高いセルで adoption の立ち上がりが加速する

## 学術的な立場

このモジュールは、現実世界の作物史・家畜史を固定再現するものではない。
生成世界に対して、文献で知られる大まかな環境ニッチと拡散の性質を移植する。

### 各局面で依拠する理論

**O（起源・初期成立中心）**

v1近似モデルの操作原則として、
「環境適地」と「人間が継続的に試行・管理しやすい地理条件」の重なりが
初期成立中心になりやすい、という前提を置く。
これは `origin_potential = niche_score * corridor_score * human_management_score`
として実装するための操作仮説である。

hilly flanks / nuclear zone 仮説（Braidwood）は、
このうち近東の穀物・家畜に関する代表的な地域仮説として参照するにとどめる。
低湿地稲作、森林縁のブタ、塊茎作物の複数成立中心などを
単一の地域仮説で説明することは意図しない。
現実の固定原産地は使わず、生成世界の地形・水文・生態から毎回導出する。

**D（拡散）**

v1近似モデルの操作仮説として、demic×cultural 混合拡散モデル（Fort ほか）の枠組みを採用する。
拡散速度は距離だけでなく地形摩擦によって変わる。
河川・低地回廊は伝導路として機能し、山脈・砂漠は摩擦として機能する。
これを `terrain_conductance` として `spread_pressure` に組み込む。
近傍伝播（cultural diffusion 的）と長距離feedbackによる非局所圧（demic的接触）の混合で表現する。

ただし、このモデルはもともと欧州新石器拡散の定量分析として提唱されたものであり、
全地球・全カテゴリへの一般化には留保が必要である。
v1では「地形が拡散速度を規定する」という操作原則のみを借用し、
demic/cultural の比率推定や人口移動の明示的モデル化は行わない。

**A（採用）**

最適採食理論（OFT）を基底に、ボーセラップ的集約化を人口密度でスケーリングする形で統合する。
環境適合度（`niche_score`）が高いほど採用の合理性が高く（OFT基底）、
人口密度が高い地域では集約化圧が加速因子として乗る（ボーセラップ的スケーリング）。
人口密度は `Population` からのfeedbackで供給され、`intensification_factor` として作用する。
`Population` 未接続時は factor = 1.0 固定で、OFT基底のみで動作する。

**M（維持）**

管理連続体（野林ほか）の慣性的側面を採用する。
`adoption` 値の収束式による慣性と、不適地での緩い `decay_rate` が、
「過去に普及していたため急には消えない」という性質を表現する。
Tier1スコープでは技術変化（二次産物革命）や制度的ロックインは扱わない。

### 参照文献・データソース

- 作物の起源地からの拡散と新環境適応のレビュー
  <https://www.annualreviews.org/content/journals/10.1146/annurev-arplant-060223-030954?TRACK=RSS>
- 作物ごとの生態要求値参照には FAO Ecocrop を使う
  例: `Hordeum vulgare`
  <https://ecocrop.apps.fao.org/ecocrop/srv/en/dataSheet?id=1232>
- 家畜の起源地・拡散の代表例として horse のレビュー
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC8550961/>
- 放牧家畜のエコゾーン分布の大枠確認として FAO の grazing systems
  <https://www.fao.org/4/x5303e/x5303e0m.htm>
- demic×cultural 混合拡散の定量モデル
  Fort et al. (2015) Demic and cultural diffusion propagated the Neolithic transition across different regions of Europe

## 関連

- `docs/reference/architecture/module_boundaries.md`
- `docs/reference/architecture/data_model.md`
- `docs/concepts/phase_control.md`
- `docs/reference/modules/ecology.md`
