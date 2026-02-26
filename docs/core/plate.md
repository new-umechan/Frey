# プレート地形（時間発展版）仕様

## 1. 目的

入力seedとparamsから、プレート運動に整合する初期地殻状態を生成し、その後の世界時間Tickに応じて地形を更新できるようにする。

本仕様は、従来の「一度だけ地形を生成する」仕様を拡張し、地殻形成期以降も地形が低頻度で継続更新される前提を定義する。

## 2. 設計方針

- 初期化と時間更新を分離する
- 地形出力とは別に、地殻内部状態を永続化する
- 1 Tickごとの更新は増分更新にする
- 時代スケール制御により、地形更新の頻度と内部反復数を変えられるようにする
- 既存の一発生成APIは互換用途として残す

## 3. 用語

- 初期化: seedとparamsから初期地殻状態を構築する処理
- 地殻状態: プレート運動、境界活動、地殻属性、標高更新に必要な内部状態
- 地形スナップショット: 描画や他サブシステム参照用の `height` などの出力
- 地形内部Tick: 地形サブシステム内の更新ステップ。`World` のTickとは一致しなくてよい

## 4. 入出力と状態

### 4.1 入力

- seed: String
- params: TerrainParams

既存の `TerrainParams` を基本としつつ、時間発展のために以下の追加パラメータ群を持てるようにする。

- `tectonic_dt`: 地形内部Tickの時間幅
- `plate_motion_gain`: プレート速度スケール
- `boundary_reclassify_interval`: 境界の再分類間隔
- `river_rebuild_interval_min`: 河川再計算の最短内部Tick間隔
- `river_rebuild_interval_max`: 河川再計算の最長内部Tick間隔
- `river_activity_high_threshold`: 高活動時の河川更新閾値
- `river_activity_low_threshold`: 低活動時の河川更新閾値
- `uplift_rate_gain`: 造山の増分係数
- `subsidence_rate_gain`: 沈降の増分係数
- `stress_relaxation_rate`: 応力緩和係数
- `isostasy_rate`: アイソスタシー緩和係数
- `subduction_age_coupling`: 海洋地殻年齢と沈み込み強度の連動係数

注意:
- 既存の境界係数や侵食係数はそのまま使い、時間発展では「1回適用の強さ」ではなく「単位時間あたりの増分率」として解釈する。

### 4.2 地形スナップショット出力（公開）

最低限、他サブシステムが参照する出力は従来と同様に保持する。

```rust
pub struct TerrainOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}
```

将来的には、活動度観測や可視化向けに追加フィールドを持てる。

- `terrain_activity`
- `boundary_activity`
- `uplift_rate`
- `subsidence_rate`

### 4.3 地殻内部状態（非公開/永続）

時間発展には、地形スナップショットとは別に内部状態が必要である。

```rust
pub struct TectonicTerrainState {
    pub mesh: SharedMeshRef,
    pub tick_internal: u64,
    pub params: TerrainParams,

    pub plate_state: Vec<PlateState>,
    pub vertex_state: Vec<VertexCrustState>,
    pub boundary_state: BoundaryState,

    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,

    pub cached_metrics: TerrainStepMetrics,
}
```

概念上の保持項目:

- プレートごとの運動状態（回転軸、角速度、活動度）
- 頂点ごとの地殻属性（海洋/大陸、年齢、厚さ、応力、浮力、侵食感受性）
- 動的境界情報（境界辺、境界タイプ、強度、直近の再分類結果）
- 地形更新の活動指標（時代遷移判定に使う）

## 5. API構成（仕様）

### 5.1 初期化API

- `init_tectonic_terrain(seed, params) -> TectonicTerrainState`

役割:
- メッシュ生成
- プレート分割
- 初期プレート属性付与
- 初期標高生成
- 初期境界地形適用
- 初回の河川/湖沼計算

### 5.2 更新API

- `step_tectonic_terrain(state, budget_ticks) -> TerrainStepMetrics`

役割:
- 地形内部Tickを `budget_ticks` 回だけ進める
- 状態を破壊的更新する（巻き戻し対応は後述のチェックポイント保存で担保する）
- 活動量などのメトリクスを返す

`budget_ticks` の定義:
- 単位は「地形内部Tickの回数」
- 物理時間換算は `budget_ticks * tectonic_dt`
- `World` から呼ぶ場合、`world.budgets.terrain` をそのまま `budget_ticks` として渡す

重要:
- `budget_ticks` は実行時間(ms)ではなく、決定的再現のための論理ステップ数である
- 比較実験や巻き戻し再生では、`budget_ticks` の列を入力条件として扱う

### 5.3 スナップショット取得API

- `snapshot_tectonic_terrain(state) -> TerrainOutput`

役割:
- 他サブシステムや描画が参照する安定した出力を返す

注意:
- `TerrainOutput` は公開スナップショットであり、単独では `TectonicTerrainState` を完全復元できない

### 5.4 チェックポイントAPI（巻き戻し用）

- `serialize_tectonic_terrain_state(state) -> Bytes | Json`
- `deserialize_tectonic_terrain_state(blob) -> TectonicTerrainState`

役割:
- 巻き戻し、分岐、保存/再開に使う完全状態の入出力
- `step_tectonic_terrain` の破壊的更新と両立させる

要件:
- 地形内部状態の完全復元ができること
- 同一チェックポイントから同一 `budget_ticks` 列を再生したとき決定的に一致すること
- `TerrainOutput` だけで復元しようとしないこと

最低限チェックポイントへ含める項目:
- `tick_internal`
- プレート運動状態（角速度、活動度、種別）
- 頂点地殻状態（年齢、厚さ、応力など）
- 動的境界状態
- `height` / `plate_id` / `river_flux` / `river_next`
- 地形サブシステム内部の乱数状態（使用している場合）

推奨チェックポイント間隔（`World.tick` 基準）:
- 地殻形成期: 10〜50 tick に1回
- 環境形成期: 5〜10 tick に1回
- 生命誕生期以降: 1〜5 tick に1回

運用メモ:
- 上限側はストレージ節約寄り、下限側は巻き戻し応答性寄り
- 分岐操作の直前/直後は、上記間隔に関係なく追加チェックポイントを作ってよい

### 5.5 互換API

既存の一発生成APIは、次の糖衣として残してよい。

- `generate_terrain(seed, params)`
  - `init_tectonic_terrain` を呼ぶ
  - 所定回数の `step_tectonic_terrain` を実行する（または0回）
  - `snapshot_tectonic_terrain` を返す

## 6. 初期化フェーズ（従来仕様の再定義）

初期化は「完成地形を作る」ではなく、「時間発展可能な初期地殻状態を作る」処理として扱う。

### 6.1 メッシュ準備

1. `generate_icosphere(level)` で `positions`, `tri_indices` を取得
2. 近傍CSRを構築
3. 球面座標を計算

### 6.2 マントル場 φ の構築

従来と同様に球面調和ベースで `phi` を生成し、z-score正規化する。

役割:
- プレートseed抽出
- 初期プレート形状の歪み
- 初期標高の大局的骨格

### 6.3 プレートseed抽出と初期分割

従来仕様のアルゴリズムを基本維持する。

- 局所極大/極小からseed候補を抽出
- 最小距離制約つきで採用
- farthest-point補完
- 多源伝播で `plate_id` を決定
- 連結性の後処理

注意:
- 初期分割は時間発展の初期条件であり、将来の境界再配置を妨げないよう、内部状態へ境界履歴を持てる構造にする。

### 6.4 プレート属性と初期運動状態

各プレートに次を設定する。

- 海洋/大陸フラグ
- 基準標高
- 基準地殻厚/密度（簡略値）
- 初期速度ではなく、球面剛体回転としての初期角速度ベクトル
- 境界活動に対する係数（剛性、変形しやすさ）

重要:
- 速度を頂点ごとに直接保持するのではなく、プレートごとの回転状態から導出できる形を基本とする。

### 6.5 初期標高と境界地形の適用

従来の境界モデルを使って初期標高を作るが、時間発展版では以下を守る。

- 過度に完成し切った地形にしない
- 後続Tickで成長・侵食できる余地を残す
- 境界由来の地形は「初期オフセット」として記録可能にする

### 6.6 初期侵食・河川

初回の河川計算と簡易侵食は任意とする。

- 高速起動重視: 初回侵食は弱くする、または省略
- 見た目重視: 従来相当の初期侵食をかける

どちらでも、時間発展の本体は後続のTick更新で行う。

## 7. Tick更新フェーズ（新設）

地形サブシステムは、`World` の1 Tickあたりに0回以上の内部Tickを実行する。

`World` との単位対応:
- `World.tick` は時代ごとの管理時間単位
- `world.budgets.terrain` は、その `World.tick` 中に実行する地形内部Tick回数
- したがって地形の進行時間は `world.budgets.terrain * tectonic_dt`

初版では、`compute_budgets(world.era, world)` は整数 `terrain` 予算を返す。
例（暫定）:
- 地殻形成期: `terrain = 8..32`
- 環境形成期: `terrain = 1..8`
- 生命/文明/歴史期: `terrain = 0..2`

上記の具体値は調整対象だが、単位は常に「内部Tick回数」に固定する。

### 7.1 更新順序（1地形内部Tick）

1. プレート運動更新
2. 境界再抽出/再分類（必要間隔で）
3. 境界由来の隆起・沈降・火山弧・リフトの増分適用
4. 応力緩和・アイソスタシー補正
5. 侵食・堆積の増分更新
6. 河川更新（毎回または間引き）
7. 活動量メトリクス更新

### 7.2 プレート運動更新

各プレートは球面上の剛体回転として扱う。

- `omega[plate]` を角速度ベクトルとする
- 頂点位置そのものは共有メッシュを固定し、まずは「速度場と境界判定」だけを更新してよい
- 境界移動を本格導入する場合は、後述の `plate_id` 再割当モードを使う

初版の実装指針:
- まずはメッシュ座標を固定し、相対速度だけで境界タイプと地形増分を更新する

### 7.3 境界再抽出/再分類

境界辺ごとに、両プレートの相対運動から毎回または一定間隔で分類する。

分類の基本:

- 法線方向相対速度 > `+eps`: 収束
- 法線方向相対速度 < `-eps`: 発散
- それ以外: 横ずれ

補足:
- 海洋/大陸の組み合わせ判定はプレート属性から求める
- `subduction_age_coupling` により、海洋地殻年齢を沈み込み強度に反映してよい

### 7.4 境界地形の増分適用

従来は1回の地形生成で大きく適用していた境界効果を、時間発展版では増分に分解する。

収束境界:
- 海溝の深化（海洋側）
- 火山弧/島弧の隆起
- 大陸衝突帯の幅広い造山

発散境界:
- リフト沈降
- 新しい海洋地殻の生成（簡略モデル）

横ずれ境界:
- 大規模な平均標高変化は抑え、粗さや線状地形を弱く付与

適用則:
- 境界距離減衰を使う
- 1内部Tickの変化量に上限を設ける
- 応力や地殻厚の状態に応じて効率を変える

### 7.5 応力緩和とアイソスタシー

境界地形の増分適用後に、地殻の緩和を入れる。

- 応力は `stress_relaxation_rate` で減衰
- 標高と地殻厚の不整合は `isostasy_rate` で緩和

目的:
- 境界近傍の過剰な標高スパイクを抑える
- 長期的に自然な地形変形へ寄せる

### 7.6 侵食・堆積（時間発展版）

侵食は従来の同期一括処理ではなく、地形内部Tickに応じて増分適用する。

- 毎内部Tickに少量実行する
- または `budget_ticks` 内で一定回数だけ実行する
- 変化量上限を守る
- 河川再計算頻度を地形更新頻度と独立に調整してよい

### 7.7 河川更新

河川は地形更新の影響を受けるため、定期的に再計算する。

本仕様では、実装時の迷いを避けるため、初版の判定ルールを固定する。

河川再計算判定タイミング:
- 各地形内部Tickの終端で判定する

使用する指標（直近内部Tickまたは `step_tectonic_terrain` 内の集計値）:
- `terrain_activity`: 正規化された標高総変化量
- `boundary_activity`: 正規化された境界活動量

合成指標:
- `river_driver = max(terrain_activity, boundary_activity)`

再計算間隔ルール（初版）:
- `river_driver >= river_activity_high_threshold` の間は、`river_rebuild_interval_min` ごとに再計算
- `river_driver <= river_activity_low_threshold` の間は、`river_rebuild_interval_max` ごとに再計算
- その中間は線形補間した間隔を使う

強制再計算条件:
- 海陸反転セル数が閾値を超えたとき
- `step_tectonic_terrain` 呼び出しの最終内部Tick

初期既定値（暫定）:
- `river_rebuild_interval_min = 1`
- `river_rebuild_interval_max = 8`
- `river_activity_high_threshold = 0.03`
- `river_activity_low_threshold = 0.005`

### 7.8 活動量メトリクス

時代遷移判定や予算配分のため、毎内部Tickまたは毎 `step_tectonic_terrain` 呼び出しで活動量を記録する。

例:
- 標高総変化量
- 境界再分類数
- 平均隆起量 / 平均沈降量
- 侵食量 / 堆積量
- 河川網変更率

定義メモ（初版）:
- `terrain_activity` は `sum(abs(delta_height)) / V` を基準に正規化する
- `boundary_activity` は境界辺ごとの相対速度指標の平均または総和を正規化する
- 正規化後の値域目標は `[0, 1]`

## 8. `plate_id` の時間発展モード

`plate_id` の扱いは段階導入できるように、仕様上2モードを定義する。

### 8.1 モードA（固定プレート領域）

- 初期化時の `plate_id` を固定する
- 境界タイプと境界活動度のみ時間更新する
- 地形は十分に動くが、境界線そのものはセル単位では移動しない

用途:
- 実装初期
- 高速動作
- 既存実装との互換性重視

### 8.2 モードB（動的プレート領域）

- 一定間隔で `plate_id` を再割当する
- プレート境界の前進・後退を表現する
- 地殻年齢、地殻種別、沈み込み履歴の更新ルールが必要

注意:
- モードBは複雑度が高いので、モードAを先に成立させる

## 9. `World` との接続

地形が時間発展する場合、`World.core` の `height` / `river_flux` / `river_next` だけでは不足する。

必要な責務分離:

- `World.core`: 他サブシステムが読む公開スナップショット
- 地形サブシステム内部状態: プレート運動と地殻更新に必要な永続状態

`World` は地形内部状態を直接保持するか、地形サブシステムオブジェクトを保持する。

巻き戻し/分岐との整合:
- `World` のキーフレームには `core` の公開状態だけでなく、地形内部状態のチェックポイントも含める
- 差分保存を行う場合も、地形内部状態の差分または再生可能なイベント列を保存する
- `TerrainOutput` のみを保存して地形内部状態を再構築する運用は不可とする

## 10. 決定性ルール（時間発展版）

- 初期化は seed + params に対して決定的である
- Tick更新は、同じ初期状態・同じ更新順序・同じ予算配分なら決定的である
- 活動量メトリクスも再現可能であることを目標とする

注意:
- 予算制御で間引き頻度を変えると結果が変わるため、比較時は更新スケジュールも入力条件として扱う

## 11. バリデーション基準

### 11.1 形状/範囲

- 各配列長が頂点数と一致
- `height` は許容範囲内（例: `[-1.5, 1.5]` の内部表現）
- `plate_id` は有効範囲内
- `river_next` は `-1` または有効頂点番号

### 11.2 動的整合性

- 境界分類が相対速度と矛盾しない
- 1内部Tickの標高変化量上限を超えない
- 応力/年齢/厚さが負値などの不正値にならない
- 河川ループは終端化または修正される

### 11.3 決定性

- 同一seed + params + 更新スケジュールで再現する
- `plate_id` 固定モードでは初期 `plate_id` が一致する

## 12. 段階導入方針（推奨）

1. モードA（固定 `plate_id`）で、境界再分類 + 地形増分更新 + 侵食の時間発展を導入する
2. 地殻年齢と沈み込み強度の連動を追加する
3. モードB（動的 `plate_id`）を導入する
4. `World` の時代スケール制御と完全統合する

## 13. 既存仕様からの移行メモ

- 従来の「プレート地形生成仕様」は、本仕様の初期化フェーズとして読み替える
- 一発生成APIはデバッグ、プリセット生成、比較実験に残す
- 実運用では、地殻形成期以降も低頻度で `step_tectonic_terrain` を継続実行する
