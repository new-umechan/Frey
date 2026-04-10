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

`clock.epoch` は、先史期以降に Domesticates を有効化するために読む。
初期成立中心シードはモジュール初回有効tickで初期化する。
FeedbackQueue で受けるのは、近傍セルの自然拡散ではなく、
移住・交易接触などで生じる長距離または非局所の追加圧だけである。

## 出力

Domesticatesは次の配列を全セル分持つ。

```rust
// u8で7種の作物をビット管理
// bit0: Wheat, bit1: Rice, bit2: Maize, bit3: Millet
// bit4: Tuber, bit5: Legume, bit6: Barley
type CropBitmap = u8;

// u8で5種の家畜をビット管理
// bit0: Cattle, bit1: Horse, bit2: Sheep, bit3: Pig, bit4: Camel
type LivestockBitmap = u8;

const N_CROPS: usize = 7;
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
  温帯寄りのコムギ類
- `Barley`
  乾燥・寒冷耐性が相対的に高いオオムギ類
- `Rice`
  高温多水・低地水利用寄りのイネ類
- `Millet`
  半乾燥・短期栽培寄りの雑穀類
- `Maize`
  暖温帯から熱帯寄りのトウモロコシ類
- `Legume`
  マメ類の代表カテゴリ
- `Tuber`
  地下器官作物の代表カテゴリ

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

- `Tuber` はジャガイモ・ヤム・キャッサバなどを厳密には分けない
- `Legume` はダイズ・エンドウ・レンズマメ・インゲン類などを束ねる
- v1 では分類学的厳密性よりも、生業分化に必要な環境ニッチ差を優先する

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
- `spread_pressure`

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

`river_bonus` は主に `Rice` と湿潤寄り `Tuber` の適地補正に使う。
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

将来的に `Settlement` や人口密度レイヤーが入る場合は、
`human_management_score` に統合して精密化してよい。

方針:

- 1カテゴリ1起源とは限らず、妥当な成立中心が複数あれば複数シードを許す
- ただしシード乱立は避けるため、上限個数を持つ
- 同一seed・同一worldなら決定的に同じ成立中心が選ばれるようにする
- 高適地でも、人間活動条件が弱い場所は初期成立中心になりにくくする
- 逆に接触条件が良くても、環境不適地だけで成立中心にはしない

### adoption 更新

`adoption` は、成立可能性と拡散圧の両方で決まる。

概念式:

```text
target_adoption =
    available_gate
  * max(origin_seed_strength, spread_pressure)

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

ここでの慣性は `current` 自体が担う。
別の内部状態として `retained_tradition` は持たない。
「過去に普及していたため急には消えない」という性質は、
`current` から `target_adoption` への収束と、
不適地での緩い `decay_rate` により表現する。

### spread_pressure

v1 では、普及圧は近接伝播を主とする。

```text
spread_pressure =
    local_neighbor_adoption
  + routed_feedback_bonus
```

- `local_neighbor_adoption`
  近傍セルまたは近傍地域の `adoption` から計算する内生拡散圧
- `routed_feedback_bonus`
  `Settlement` が前tickに積む追加圧

feedback の責務は次で固定する。

- 近傍セルからの通常拡散は `Domesticates` 自身が現在tickの近傍 `adoption` を読んで計算する
- `Domesticates` 自身は自分の inbox に feedback を積まない
- `routed_feedback_bonus` は `Settlement` が、移住・交易接触・定住ネットワーク経由の持ち込みを表すときだけ積む
- この feedback は `FeedbackEntry.target_module = ModuleId::Domesticates` で配送する

概念例:

```rust
FeedbackEntry {
    target_module: ModuleId::Domesticates,
    kind: FeedbackKind::DomesticatesSpread {
        cell: target_cell,
        crop_delta: [f32; N_CROPS],
        livestock_delta: [f32; N_LIVESTOCK],
    },
}
```

これにより、先史期の初期段階でも `Settlement` の成熟を前提にせず、
Domesticates単体で起源地からの緩い拡散を表現できる。

## 種ごとのニッチ方針

具体的なパラメータ値は実装時に調整するが、
仕様として必要な環境方向性は次で固定する。

### 作物

- `Wheat`
  温帯寄り。過湿より中庸な降水を好み、低木被覆または開放地で高い
- `Barley`
  `Wheat` より寒冷・乾燥側まで成立しやすい。高地耐性も相対的に高い
- `Rice`
  高温多水で、河川・低地・湿地近接で強い補正を与える
- `Millet`
  半乾燥・短い生育期間向き。草地・疎林でも成立しやすい
- `Maize`
  暖温帯から熱帯寄り。低温で強く不利
- `Legume`
  中庸な環境で広く成立する補助作物群として扱う
- `Tuber`
  水分要求は比較的高いが、種内差が大きいためv1では広めに許容する

### 家畜

- `Cattle`
  草地・サバンナ・開放地で高い
- `Sheep`
  乾燥・半乾燥・粗放草地で高い
- `Pig`
  森林縁・湿潤域・農耕近接で高い
- `Horse`
  開放草地・ステップで高く、密林で低い
- `Camel`
  高温乾燥・疎植生で高い

## パラメータ管理

Domesticatesのパラメータは、将来的に専用設定ファイルへ切り出せる形で
カテゴリ別にまとめて管理する。

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
- `origin_count_limit`
- `origin_seed_strength`
- `growth_rate`
- `decay_rate`

値そのものはコードに埋め込んでもよいが、
種ごとの閾値・最適値・更新率は一箇所に集約し、散在させない。

## テスト観点

最低限、次のシナリオを満たすこと。

1. 高温多水低地で `Rice.available` が高く、乾燥高地より有利になる
2. 半乾燥冷涼セルで `Barley` と `Millet` が `Rice` より有利になる
3. 乾燥開放地で `Camel` / `Sheep` が高く、`Pig` が低くなる
4. 起源地シードが全カテゴリで0件にならない
5. 低適地セルが起源地に選ばれない
6. 適地でも孤立セルは `adoption` が即座に上がらない
7. 不適地へ入ると `adoption` は即座にゼロにならず、遅れて減衰する
8. `Subsistence` は引き続き `crop_adoption` / `livestock_adoption` のみを読める

## 学術的な根拠の扱い

このモジュールは、現実世界の作物史・家畜史を固定再現するものではない。
生成世界に対して、文献で知られる大まかな環境ニッチと拡散の性質を移植する。

参照の軸:

- 作物の起源地からの拡散と新環境適応のレビュー  
  https://www.annualreviews.org/content/journals/10.1146/annurev-arplant-060223-030954?TRACK=RSS
- 作物ごとの生態要求値参照には FAO Ecocrop を使う  
  例: `Hordeum vulgare`  
  https://ecocrop.apps.fao.org/ecocrop/srv/en/dataSheet?id=1232
- 家畜の起源地・拡散の代表例として horse のレビュー  
  https://pmc.ncbi.nlm.nih.gov/articles/PMC8550961/
- 放牧家畜のエコゾーン分布の大枠確認として FAO の grazing systems  
  https://www.fao.org/4/x5303e/x5303e0m.htm

関連:

- `docs/architecture/module_boundaries.md`
- `docs/architecture/data_model.md`
- `docs/architecture/phase_control.md`
- `docs/modules/ecology.md`
