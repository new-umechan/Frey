# プレート地形仕様

## 1. 目的

入力seedとparamsから、プレート運動に整合する初期地殻状態を生成し、その後の世界時間Tickに応じて地形を更新できるようにする。

## 2. 設計方針

- 初期化と時間更新を分離する
- 地形出力とは別に、地殻内部状態を永続化する
- 1 Tickごとの更新は増分更新にする
- 時代スケール制御により、地形更新の頻度と内部反復数を変えられるようにする
- 地形の状態名（大地溝帯）などは保持せず、ルールから地形が発生するようにする

### 2.1 採用モデル
連続体力学を元にすると重いため、応力伝播モデルを用いて作る。
また、長期的なスケールの再現（ウィルソンサイクル、5億年単位）には
マントル熱ダイナミクスを再現すること

大陸地殻は熱が逃げにくく、海洋地殻は熱が逃げやすいように。

弾性波方程式を解くのではなく弾性薄板モデルを用いる

### 2.2 マントルとプレートの交互作用

以下は実際の式ではなく、動きの理解の参考のため。

毎tick
	mantle_heat[cell] += heat_input  # 定数
	mantle_heat[cell] -= heat_loss * discharge_rate(crust_type[cell])

crust_typeによる放熱率
	大陸地殻: discharge_rate = 0.1  # 熱が逃げにくい
	海洋地殻: discharge_rate = 1.0  # 熱が逃げやすい

プルーム発生
if mantle_heat[cell] > plume_threshold:
    uplift_force[cell] = (mantle_heat - plume_threshold) * plume_gain
    mantle_heat[cell] *= heat_release_rate  # 熱を放出

注)ここで使われている式の説明
heat_input: 定数
discharge_rate: 大陸/海洋で2値
plume_threshold: 分裂トリガーの閾値
plume_gain: プルーム力の強さ
heat_release_rate: プルーム発生時の放熱率

### 2.3 処理の流れ
1. マントル熱場の更新
	- 熱蓄積・放熱
	- プルーム判定 → uplift_force生成
        ↓
2. 境界タイプ判定・再分類
        ↓
3. プレート運動方程式でωを更新
        ↓
4. 応力伝播（弾性薄板モデル）
   - 境界タイプ別に圧縮・引張・せん断応力を生成
   - uplift_forceも応力として追加
        ↓
5. 境界で応力テンソルを生成
		↓
6. 火山モデル
        ↓
7. 各セルの標高・地殻厚を更新
        ↓
8. 侵食・堆積・河川更新（詳細はdocs/core/errosion.md）
        ↓
9. 活動量メトリクス更新

## 3. 具体的な仕様

### 3.0 プレートの動き

τの各項を境界タイプごとに条件付きで計算

```text
I * dω/dt = τ_slab + τ_ridge + τ_mantle + τ_collision
```

τ_slab     = Σ(Subduction境界の辺) slab_pull_per_edge
τ_ridge    = Σ(Ridge/Rift境界の辺) ridge_push_per_edge
τ_collision = Σ(Collision境界の辺) collision_resistance_per_edge
τ_mantle   = -ω * plate_area * mantle_drag  # 全プレート共通

PassiveMarginは力を生成しないので、そのプレートはτ_mantleによる減衰だけで動く。つまり慣性で動き続けて、徐々に減速する。


### 3.1 プレート分裂、合体

#### 3.1.1 フェーズ

フェーズ1: 大地溝帯
  - uplift_forceが発生（大陸にホットスポットが形成される）
  - 標高が下がり、地溝帯地形が形成される
  - boundary_typeは「pre-rift」

フェーズ2: 海洋誕生
  - riftingセルが「oceanic_young」に変わる
  - 海嶺として登録される
  - 両側のplate_idが分離される
  - リッジプッシュ開始

フェーズ3: 海洋拡大
  - 海嶺から両側に対称的にoceanic地殻が付加される
  - 海洋地殻の年齢・密度が時間とともに増加

#### 3.1.2 フェーズ遷移条件

フェーズ1→2（大地溝帯→海洋誕生）
riftingセルの平均標高 < rift_to_ocean_height_threshold
かつ
riftingセルが海面下に達した割合 > rift_ocean_ratio_threshold
地溝帯が十分に沈降して海水が入り込んだら海洋誕生

フェーズ2→3（海洋誕生→海洋拡大）
ridge_cellsの両側plate_idが確定している
かつ
両側プレートの相対発散速度 > ridge_spread_threshold
海嶺が安定して、両側が離れ始めたら拡大フェーズへ。

#### 3.1.3 ウィルソンサイクル上の状態

あくまでも名称をつけているだけで、この状態が保存されているわけではない。

大地溝帯: stress > 0（引張）かつ thickness が薄くなっている地域
海洋誕生: Continental → Oceanic への crust_type 変化（標高が海面下に達したとき）
海嶺: 発散境界辺のうち、両側が若い海洋地殻（age が低い）
沈み込み: 収束境界で density が高い海洋地殻が隣接大陸と接している状態

### 3.2 マントル熱場の仕様

これを地殻成形期には毎回計算する。
期の比に応じて、計算回数は疎にする
1. 熱蓄積・放熱
2. 熱拡散
3. プルームの処理

熱拡散はとなりのセルのみ計算する。

```rust
mantle_heat[cell] += heat_input
mantle_heat[cell] -= heat_loss * discharge_rate(crust_type[cell])
for neighbor in neighbors[cell]:
    heat_diff += (mantle_heat[neighbor] - mantle_heat[cell]) * diffusion_rate
mantle_heat[cell] += heat_diff
```

### 3.3 応力伝播モデルの仕様

境界で発生した応力が、隣接セルへ減衰しながら伝播する。

```rust
stress[cell] += boundary_stress * attenuation(distance) * (1 / rigidity[cell])
```
boundary_stress: 境界タイプ（収束・発散・横ずれ）から生成
attenuation(distance): 距離に応じた減衰
rigidity: 地殻の硬さ。硬いほど応力が伝わりにくい

境界タイプ別の境界応力:
- Subduction / Collision: 圧縮応力を与える
- Ridge / Rift: 引張応力を与える
- Transform: せん断応力を与える
- PassiveMargin: 境界応力はほぼゼロ（必要なら微小ノイズのみ）

PassiveMargin補足:
- 大陸棚での堆積は地形側の長期更新で扱う
- 応力源としては中立に扱う

#### 応力の形式

2x2行列として保持
```rust
struct StressTensor {
	xx: f32,  // 東西方向の応力
	yy: f32,  // 南北方向の応力
	xy: f32,  // せん断応力
}
```

#### 伝播
D * ∇⁴w = q(x,y)

D: 地殻の曲げ剛性（rigidityから導出）
w: 地殻の撓み（変位）
q: 外力（境界応力・プルーム力）

### 3.4 火山モデルの仕様

## 4. 入出力と状態

### 4.1 入力

- seed: String
- params: TerrainParams

既存の `TerrainParams` を基本としつつ、時間発展のために以下の追加パラメータ群を持てるようにする。

- `plate_motion_gain`: プレート速度スケール
- `boundary_reclassify_interval`: 境界の再分類間隔
- `river_rebuild_interval_min`: 河川再計算の最短Tick間隔
- `river_rebuild_interval_max`: 河川再計算の最長Tick間隔
- `river_activity_high_threshold`: 高活動時の河川更新閾値
- `river_activity_low_threshold`: 低活動時の河川更新閾値
- `uplift_rate_gain`: 造山の増分係数
- `subsidence_rate_gain`: 沈降の増分係数
- `stress_relaxation_rate`: 応力緩和係数
- `isostasy_rate`: アイソスタシー緩和係数
- `subduction_age_coupling`: 海洋地殻年齢と沈み込み強度の連動係数
- `subduction_initiation_threshold`: PassiveMarginから沈み込み開始へ移行する最小海洋地殻年齢
- `subduction_density_threshold`: PassiveMarginから沈み込み開始へ移行する最小密度

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
    pub params: TerrainParams,

    pub plate_state: Vec<PlateState>,
    pub vertex_state: Vec<VertexCrustState>,
    pub boundary_state: BoundaryState,

    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
	pub mantle_heat: Vec<f32>,  // セルごとの熱量 [0, 1]正規化

    pub cached_metrics: TerrainStepMetrics,
}

struct VertexCrustState {
    crust_type: CrustType,  // Continental / Oceanic のみ
    thickness: f32,
    density: f32,
    age: f32,
    stress: f32,           // 引張(+) / 圧縮(-)
    temperature: f32,      // マントル熱場から受け取る
    rigidity: f32,         // 地殻の硬さ
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

- `step_tectonic_terrain(state) -> TerrainStepMetrics`

役割:
- 1回の地形更新を実行する
- 状態を破壊的更新する（巻き戻し対応は後述のチェックポイント保存で担保する）
- 活動量などのメトリクスを返す

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
- チェックポイントの作成タイミングは呼び出し側が管理する

要件:
- 地形内部状態の完全復元ができること
- 同一チェックポイントから同一更新列を再生したとき決定的に一致すること
- `TerrainOutput` だけで復元しようとしないこと

最低限チェックポイントへ含める項目:
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
- 地形モジュールは自動で定期チェックポイントを作成しない

## 6. 初期化フェーズ

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

## 7. Tick更新フェーズ

地形サブシステムは、`World` の1 Tickごとに1回の地形更新を実行する。

`World` との単位対応:
- `World.tick` は時代ごとの管理時間単位
- 地形更新は `World.tick` に同期して1回実行する

### 7.1 更新順序（1回の地形更新）

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

```rust
enum BoundaryType {
	// 発散
	Ridge,           // 海嶺（海洋地殻生成）
	Rift,            // 大地溝帯（分裂途中）
	
	// 収束
	Subduction,      // 海洋プレートが沈み込む
	Collision,       // 大陸同士の衝突
	
	// 横ずれ
	Transform,       // トランスフォーム断層
	
	// 中立
	PassiveMargin,   // 受動的大陸縁辺（沈み込みなし）
}
```

PassiveMarginの応力への影響
- 境界応力はほぼゼロ
- 堆積物が大陸棚に積み重なる（将来の地形に影響）
- 将来的にSubductionに転換する可能性がある（大西洋がいずれ閉じるように）

転換条件（PassiveMargin → Subduction）:
```text
oceanic_crust.age > subduction_initiation_threshold
and
oceanic_crust.density > subduction_density_threshold
```
上記を満たした境界は、再分類時にSubductionとして扱ってよい。

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
- 1回の地形更新あたりの変化量に上限を設ける
- 応力や地殻厚の状態に応じて効率を変える

### 7.5 応力緩和とアイソスタシー

境界地形の増分適用後に、地殻の緩和を入れる。

- 応力は `stress_relaxation_rate` で減衰
- 標高と地殻厚の不整合は `isostasy_rate` で緩和

目的:
- 境界近傍の過剰な標高スパイクを抑える
- 長期的に自然な地形変形へ寄せる

### 7.8 活動量メトリクス

時代遷移判定や予算配分のため、毎 `step_tectonic_terrain` 呼び出しで活動量を記録する。

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

## 10. 決定性ルール

- 初期化は seed + params に対して決定的である
- Tick更新は、同じ初期状態・同じ更新順序・同じ予算配分なら決定的である
- 活動量メトリクスも再現可能であることを目標とする

## 11. バリデーション基準

### 11.1 形状/範囲

- 各配列長が頂点数と一致
- `height` は許容範囲内（例: `[-1.5, 1.5]` の内部表現）
- `plate_id` は有効範囲内
- `river_next` は `-1` または有効頂点番号

### 11.2 動的整合性

- 境界分類が相対速度と矛盾しない
- 応力/年齢/厚さが負値などの不正値にならない
- 河川ループは終端化または修正される

### 11.3 決定性

- 同一seed + params + 更新スケジュールで再現する
- `plate_id` 固定モードでは初期 `plate_id` が一致する
