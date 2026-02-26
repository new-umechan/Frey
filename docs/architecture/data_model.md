# World仕様（初版）

## 1. 目的

`World` は、地形・河川・気候・生態・文明の各サブシステムを束ねる実行単位である。
本仕様は、`World` が何を保持し、どの順序で更新されるかを定義する。

本書では、詳細アルゴリズムではなく、構造と責務の分割を先に固定する。

## 2. 設計方針

- 常に必要な地形・河川の状態は `core` に置く
- 時代に応じて必要になる状態は `layers` に置く
- 各レイヤーは遅延生成可能にする
- 各サブシステムの更新頻度は、時代スケール制御で決める
- `World` は「計算の調停役」であり、各サブシステムの内部ロジックは別モジュールへ分ける

## 3. 構造（初版決定）

### 3.1 `World` 全体

```rust
pub struct World {
    pub tick: u64,
    pub era: EraKind,
    pub mesh: WorldMesh,
    pub core: CoreCells,
    pub layers: HashMap<LayerKind, CellLayer>,
    pub budgets: SubsystemBudgets,
}
```

初版では `layers` に `HashMap<LayerKind, CellLayer>` を採用する。
理由は、期に応じた遅延生成と未生成状態の表現が簡単で、実装の立ち上がりが速いため。
性能上の問題が出た場合は、固定スロット構造へ移行を検討する。

時間発展する地形を導入する場合は、上記に加えて「地形内部状態（プレート運動、境界活動、地殻属性など）」を保持する拡張スロットを `World` に追加する。
この内部状態は `core` の公開スナップショットとは分けて扱う。

### 3.2 常時存在するコアレイヤー

```rust
pub struct CoreCells {
    pub height: Vec<f32>,
    pub plate_id: Vec<u16>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}
```

`core` は全時代で存在する。
地形・河川は、気候・生態・文明の入力基盤になるため、`layers` へ入れない。

補足:
- `core` は他サブシステム参照用の公開状態を置く
- プレート運動や地殻年齢などの更新用内部状態は `core` に直接詰め込まず、地形サブシステム側の永続状態として保持する

### 3.3 期に応じて追加されるレイヤー

```rust
pub enum LayerKind {
    Climate,
    Ecology,
    Civilization,
}
```

```rust
pub enum CellLayer {
    Climate(ClimateLayer),
    Ecology(EcologyLayer),
    Civilization(CivilizationLayer),
}
```

`CellLayer` を enum にすることで、`LayerKind` と実体の取り違えを防ぐ。

### 3.4 各レイヤーの最小構造（初版）

```rust
pub struct ClimateLayer {
    pub temp: Vec<f32>,
    pub rain: Vec<f32>,
}

pub struct EcologyLayer {
    pub habitability: Vec<f32>,
    pub productivity: Vec<f32>,
}

pub struct CivilizationLayer {
    pub population: Vec<f32>,
    pub state_id: Vec<u32>,
}
```

初版では各レイヤーを最小2指標程度に絞る。
詳細指標は後から増やす。

## 4. `WorldMesh` の役割

セル状態ではなく、全レイヤーで共有されるメッシュ情報は `WorldMesh` に分離する。

```rust
pub struct WorldMesh {
    pub positions: Vec<[f32; 3]>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
}
```

これにより、`core` や `layers` を差し替えても、メッシュ共有情報を再構築せずに済む。

## 5. 時代とレイヤー生成タイミング（暫定）

各時代は排他的なモードではないが、初回に必要となるレイヤーの生成タイミングの目安は持つ。

- 地殻形成期
  - `core` のみで開始
  - `Climate` / `Ecology` / `Civilization` は未生成でもよい
- 環境形成期
  - `Climate` を生成（未生成なら）
- 生命誕生期
  - `Ecology` を生成（未生成なら）
- 文明成立期
  - `Civilization` を生成（未生成なら）
- 歴史展開期
  - 全レイヤー共存

重要なのは、生成後のレイヤーは後続時代でも保持し続けること。

## 6. Worldの責務

`World` の責務は次の通り。

- 世界時間 `tick` の進行
- 現在の時代 `era` の管理
- 時代スケール制御にもとづく更新予算 `budgets` の算出
- 必要レイヤーの遅延生成
- 各サブシステム更新の呼び出し順序の管理
- サブシステム間の入出力の受け渡し

`World` 自身は、気候式や生態式の詳細を持たない。
それらは各サブシステム実装に委譲する。

## 7. 1 Tick 処理（暫定）

### 7.1 概要

`World` の 1 Tick は、時代の管理時間を1つ進める処理である。
各サブシステムの内部更新回数は、`budgets` に従って決める。

### 7.2 流れ（擬似コード）

```rust
pub fn step_world(world: &mut World) {
    world.budgets = compute_budgets(world.era, &world);

    ensure_required_layers(world);

    run_terrain_step(world, world.budgets.terrain);
    run_river_step(world, world.budgets.river);
    run_climate_step(world, world.budgets.climate);
    run_ecology_step(world, world.budgets.ecology);
    run_civilization_step(world, world.budgets.civilization);

    update_era_transition(world);
    world.tick += 1;
}
```

### 7.3 呼び出し順の考え方

順序は次を推奨する。

- 地形
- 河川
- 気候
- 生態
- 文明

理由:
- 地形/河川が環境条件の基盤になる
- 気候は河川・地形に依存しやすい
- 生態は気候・河川に依存する
- 文明は生態・河川・地形に依存する

将来的には、同一 Tick 内で複数回の反復（例: 河川と気候の交互更新）を導入してよい。

## 8. 時代スケール制御との接続

`World` は、時代スケール制御の結果として「各サブシステムにどれだけ計算予算を配るか」を受け取る。

初版では、予算は単純な整数回数でよい。

```rust
pub struct SubsystemBudgets {
    pub terrain: u32,
    pub river: u32,
    pub climate: u32,
    pub ecology: u32,
    pub civilization: u32,
}
```

後で必要なら次を追加する。

- 各サブシステムの内部時間幅
- 実行コスト上限（ms）
- スキップ確率 / 低優先度キュー

## 9. 既存実装との接続方針（初版）

現状コードとの接続は、次の順で進める。

1. `World` に河川非同期オートマトン状態を保持する
2. `run_river_step` から非同期stepを呼ぶ
3. `Climate` / `Ecology` / `Civilization` はダミー更新で枠組みだけ接続する
4. その後、各レイヤーの実更新を段階的に実装する

この順序により、既存の地形・河川資産を使いながら `World` の実行ループを先に成立させられる。

## 10. 決定事項と未決事項

### 決定事項（初版）

- `World` は `mesh + core + layers + era + budgets + tick` を持つ
- `core` には地形・河川の常時必要状態を置く
- `layers` は `HashMap<LayerKind, CellLayer>` とし、遅延生成を許可する
- `CellLayer` は enum にして型安全を優先する
- `river_next` は現行実装との整合のため `i32` を使う
- 時間発展地形を導入する場合、`core` とは別に地形内部状態を保持する

### 未決事項

- `World` に河川非同期オートマトン状態を直接持つか、別オブジェクト参照にするか
- 時間発展地形の内部状態を `World` に直接持つか、別オブジェクト参照にするか
- `core.river_flux` / `core.river_next` と河川オートマトン内部状態の同期タイミング
- `layers` を将来的に固定スロット構造へ移行するか
- `state_id` を `CivilizationLayer` に置くか、文明グラフ側に分離するか
- `tick` と各サブシステム内部時間の対応づけ方法

### 10.1 巻き戻し/分岐時の保存単位（追記）

時間発展地形を導入する場合、巻き戻し用キーフレームには `core` の公開状態だけでなく、地形内部状態の完全チェックポイントを含める。

理由:
- `core.height` / `core.plate_id` / `core.river_flux` / `core.river_next` だけでは、プレート運動状態、地殻年齢、応力、動的境界状態などを復元できない
- そのため `TerrainOutput` 相当の公開スナップショットのみでは再開できない

差分保存を行う場合も、次のどちらかを満たす必要がある。

- 地形内部状態の差分を保存する
- 地形内部状態を決定的に再生できる更新イベント列（予算列を含む）を保存する

推奨キーフレーム間隔（`World.tick` 基準、時間発展地形あり）:
- 地殻形成期: 10〜50 tick に1回
- 環境形成期: 5〜10 tick に1回
- 生命誕生期以降: 1〜5 tick に1回

補足:
- 分岐点の前後は追加キーフレームを優先する
- 巻き戻し操作の多い時代では、上記レンジの下限寄りを使う

---

本仕様は、`docs/architecture/overview.md` の「並列進行 + 時代スケール制御」を実装へ落とすための構造メモである。
