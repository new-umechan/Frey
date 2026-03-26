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

弾性波方程式を解くのではなく弾性薄板モデルを用いる。

### 2.2 マントルとプレートの交互作用

以下は実際の式ではなく、動きの理解の参考のため。

毎tick

```text
mantle_heat[cell] += heat_input
mantle_heat[cell] -= heat_loss * discharge_rate(crust_type[cell])
```

crust_typeによる放熱率
	大陸地殻: discharge_rate = 0.1  # 熱が逃げにくい
	海洋地殻: discharge_rate = 1.0  # 熱が逃げやすい

プルーム発生

```text
if mantle_heat[cell] > plume_threshold:
    uplift_force[cell] = (mantle_heat - plume_threshold) * plume_gain
    mantle_heat[cell] *= heat_release_rate
```

注)ここで使われている式の説明
heat_input: 定数
discharge_rate: 大陸/海洋で2値
plume_threshold: 分裂トリガーの閾値
plume_gain: プルーム力の強さ
heat_release_rate: プルーム発生時の放熱率

### 2.3 処理の流れ

1. マントル熱場の更新

   - 熱蓄積・放熱
   - 熱拡散
   - プルーム判定 → uplift_force生成
2. プレート運動方程式でωを更新
3. 地殻属性の移流と境界通過処理
4. 境界タイプ判定・再分類
5. 応力伝播（弾性薄板モデル）

   - 境界タイプ別に圧縮・引張・せん断応力を生成
   - uplift_forceも応力として追加
6. 火山モデル
7. 各セルの標高・地殻厚を更新

   - 構造隆起
   - 構造沈降
   - 海洋熱沈降
8. 侵食・堆積の算出と地形反映（算出はHydrologyステージ、反映はHydrologyステージ直後）
9. アイソスタシー調整
10. 活動量メトリクス更新

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

#### 3.0.1 海洋地殻年齢と密度

海洋地殻の密度は年齢依存とする。

```text
age_norm = clamp(age / age_ref, 0, 1)

density_ocean(age)
= oceanic_base_density + age_density_gain * sqrt(age_norm)
```

- 大陸地殻 density は固定値を用いる
- 海洋地殻 density のみ age 依存で増加する
- `age_ref` で成熟海洋地殻の基準年齢を表す

#### 3.0.2 スラブプルとロールバック

ロールバックは独立した力源ではなく、`τ_slab` の内訳として扱う。

```text
slab_pull_mag
= edge_length
- max(0, density_ocean(age) - mantle_density)
- slab_depth_est
- g_eff
```

ここで `slab_depth_est` は、沈み込み境界の各edgeに対して年齢と収束履歴から求める無次元深度指数とする。

```text
slab_depth_est
= subduction_depth_gain * age_norm * convergence_memory
```

`convergence_memory` は各Subduction境界edgeごとに持つ履歴状態であり、現在の収束速度ではなく継続的な収束の強さを表す。

```text
convergence_memory[e] +=
  (convergence_speed_norm[e] - convergence_memory[e]) * convergence_memory_rate
```

さらに、海溝沿いの局所差を残しつつ数値ノイズを抑えるため、更新後に隣接edge間で弱い空間平滑化を行う。

```text
convergence_memory_smooth[e] =
  lerp(
    convergence_memory[e],
    mean(neighbor_edge_memories),
    convergence_memory_spatial_smooth
  )
```

スラブの立ちやすさは密度差から近似する。

```text
dip_factor
= clamp(
    (density_ocean(age) - mantle_density) / dip_density_scale,
    0,
    1
  )
```

ロールバック配分率は以下とする。

```text
rollback_fraction
= clamp(
    rollback_gain
    * age_norm
    * dip_factor
    * slab_depth_est
    * (1 - convergence_speed_norm * rollback_suppression),
    0,
    rollback_fraction_max
  )
```

最終的に `τ_slab` は収束成分とロールバック成分へ分配する。

```text
τ_slab
= Σ_boundary_edges [
    slab_pull_mag
    * (
        (1 - rollback_fraction) * n_conv
        + rollback_fraction * n_roll
      )
  ]
```

- `n_conv`: 収束方向
- `n_roll`: 海溝ヒンジ後退方向

`rollback_fraction` が `rollback_threshold` を超えたedgeでは、背弧側へ引張応力を加える。

```text
backarc_tension
= slab_pull_mag * rollback_fraction * backarc_tension_gain
```

この応力は大陸側セルへ距離減衰つきで伝播し、既存の Rift 形成条件へ接続する。

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
for neighbor in neighbors[cell] {
    heat_diff += (mantle_heat[neighbor] - mantle_heat[cell]) * diffusion_rate
}
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

Subduction境界で `rollback_fraction > rollback_threshold` のedgeでは、背弧側に追加の引張応力を付与する。これによりロールバック駆動の後弧拡張を表現する。

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

```text
D * ∇⁴w = q(x,y)
```

D: 地殻の曲げ剛性（rigidityから導出）
w: 地殻の撓み（変位）
q: 外力（境界応力・プルーム力）

### 3.4 火山モデルの仕様

火山活動は境界タイプとマントル熱場から導出される中間状態量 `volcanism` として扱い、その後に標高・地殻厚へ反映する。

#### 3.4.1 火山タイプ

- 島弧火山: Subduction境界の大陸側。スラブ脱水による融点低下。
- 海嶺火山: Ridge境界。減圧融解。
- ホットスポット火山: plume閾値を超えたセル。マントルプルームによる局所加熱。
- 後弧火山: rollbackに伴う背弧引張域。脱水と減圧融解の複合。

#### 3.4.2 島弧火山

```text
if boundary_type == Subduction {
    arc_volcanism = slab_flux * arc_volcanism_gain
}
```

- `slab_flux` は沈み込みedgeの密度差・収束速度・深度指数から導く
- 島弧火山帯はSubduction境界の大陸側一定距離に分布させる

#### 3.4.3 海嶺火山

```text
if boundary_type == Ridge {
    ridge_volcanism = spreading_rate * ridge_volcanism_gain
}
```

- 発散速度が速いほど活発
- 新しい海洋地殻生成と連動し、`age = 0` とする

#### 3.4.4 ホットスポット火山

```text
if mantle_heat[cell] > plume_threshold {
    hotspot_volcanism = (mantle_heat[cell] - plume_threshold) * hotspot_volcanism_gain
}
```

- uplift_forceと同時に局所火山活動を発生させる
- 大陸上ではリフト形成の誘因になりうる

#### 3.4.5 後弧火山

```text
if rollback_fraction > rollback_threshold {
    backarc_volcanism = rollback_fraction * backarc_volcanism_gain
}
```

- 背弧引張域で発生
- 後弧盆地形成初期の火山弧分岐を表現する

#### 3.4.6 地形への反映

各火山活動量は合算して公開用 `volcanism` を構成する。

```text
volcanism = arc_volcanism + ridge_volcanism + hotspot_volcanism + backarc_volcanism
```

標高と地殻厚には以下のように反映する。

```text
height[cell] += volcanism * volcanic_uplift_gain
thickness[cell] += volcanism * volcanic_thickening_gain
```

海嶺火山は新規海洋地殻生成と一体で扱うため、`oceanic_initial_thickness` と `oceanic_base_density` を再設定してよい。

### 3.5 標高更新モデル

時間発展時の標高更新は、単一の「沈降」係数でまとめず、次の独立した項として扱う。

- 構造起伏変化
  - 圧縮応力による隆起
  - 張力場による構造沈降
  - 火山活動による隆起と厚化
- 海洋熱沈降
  - 海洋地殻のみ対象
  - 地殻年齢の増加に応じた長期的沈降
- アイソスタシー調整
  - 地殻厚と密度差に基づく平衡高度への緩和
- 侵食・堆積反映
  - `Hydrology` が算出した侵食量・堆積量を `Geology` が標高と地殻厚へ反映する

擬似式:

```text
height_next
= height
+ tectonic_uplift
- tectonic_subsidence
- thermal_subsidence
+ erosion_deposition_delta
+ isostatic_adjustment
```

ここでの各項は次の意味を持つ。

- `tectonic_uplift`: 圧縮応力と火山活動による隆起
- `tectonic_subsidence`: 張力場やリフト形成に伴う構造沈降
- `thermal_subsidence`: 海洋地殻の冷却と高密度化に伴う沈降
- `erosion_deposition_delta`: 侵食量と堆積量の差分
- `isostatic_adjustment`: 平衡高度との差を埋める緩和項

海陸比を目標値へ毎tick補正する処理は、この地学モデルの正式な更新項には含めない。
必要なら実装上の初期正規化や数値安定化として別レイヤーで扱う。

## 4. 入出力と状態

### 4.1 入力

- seed: String
- params: GeologyParams

既存の `GeologyParams` を基本としつつ、時間発展のために以下の追加パラメータ群を持てるようにする。

- `plate_motion_gain`: プレート速度スケール
- `boundary_reclassify_interval`: 境界の再分類間隔
- `river_rebuild_interval_min`: 河川再計算の最短Tick間隔
- `river_rebuild_interval_max`: 河川再計算の最長Tick間隔
- `river_activity_high_threshold`: 高活動時の河川更新閾値
- `river_activity_low_threshold`: 低活動時の河川更新閾値
- `tectonic_uplift_gain`: 圧縮応力と火山活動を標高隆起へ変換する係数
- `tectonic_subsidence_gain`: 張力場を構造沈降へ変換する係数
- `thermal_subsidence_gain`: 海洋地殻年齢を熱沈降へ変換する係数
- `stress_relaxation_rate`: 応力緩和係数
- `isostatic_adjustment_rate`: アイソスタシー調整の緩和係数
- `subduction_age_coupling`: 海洋地殻年齢と沈み込み強度の連動係数
- `subduction_initiation_threshold`: PassiveMarginから沈み込み開始へ移行する最小海洋地殻年齢
- `subduction_density_threshold`: PassiveMarginから沈み込み開始へ移行する最小密度
- `age_ref`
- `oceanic_base_density`
- `age_density_gain`
- `mantle_density`
- `rollback_gain`
- `rollback_suppression`
- `rollback_fraction_max`
- `rollback_threshold`
- `backarc_tension_gain`
- `dip_density_scale`
- `subduction_depth_gain`
- `convergence_memory_rate`
- `convergence_memory_spatial_smooth`
- `volcanic_uplift_gain`
- `volcanic_thickening_gain`
- `arc_volcanism_gain`
- `ridge_volcanism_gain`
- `hotspot_volcanism_gain`
- `backarc_volcanism_gain`
- `erosion_thickness_coupling`
- `deposition_thickness_coupling`

注意:
- 既存の境界係数や侵食係数はそのまま使い、時間発展では「1回適用の強さ」ではなく「単位時間あたりの増分率」として解釈する。

### 4.2 地形スナップショット出力（公開）

最低限、他サブシステムが参照する出力は従来と同様に保持する。

```rust
pub struct GeologyOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
    pub volcanism: Vec<f32>,
    pub vertex_buoyancy: Vec<f32>,
}
```

### 4.3 地殻内部状態（非公開/永続）

時間発展には、地形スナップショットとは別に内部状態が必要である。

```rust
pub struct TectonicTerrainState {
    pub mesh: SharedMeshRef,
    pub params: GeologyParams,

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
    arc_volcanism: f32,
    ridge_volcanism: f32,
    hotspot_volcanism: f32,
    backarc_volcanism: f32,
}

struct BoundaryEdgeInternal {
    convergence_memory: f32,
}

struct BoundaryDynamicsState {
    edge_pairs: Vec<[u32; 2]>,
    edge_internal: Vec<BoundaryEdgeInternal>,
    slab_convergence_component: Vec<f32>,
    slab_rollback_component: Vec<f32>,
}
```

`BoundaryEdgeInternal` は各境界edgeの内部履歴（`convergence_memory`）のみを保持する。
`edge_pairs` と `slab_*_component` は `BoundaryDynamicsState` 側で管理する。

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

- `snapshot_tectonic_terrain(state) -> GeologyOutput`

役割:
- 他サブシステムや描画が参照する安定した出力を返す

注意:
- `GeologyOutput` は公開スナップショットであり、単独では `TectonicTerrainState` を完全復元できない

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
- `GeologyOutput` だけで復元しようとしないこと

最低限チェックポイントへ含める項目:
- プレート運動状態（角速度、活動度、種別）
- 頂点地殻状態（年齢、厚さ、応力など）
- 動的境界状態
- `height` / `plate_id` / `river_flux` / `river_next`
- 地形サブシステム内部の乱数状態（使用している場合）

推奨チェックポイント間隔（`World.tick` 基準）:
- 地殻形成期: 10〜50 tick に1回
- 環境形成期: 5〜10 tick に1回
- 先史期以降: 1〜5 tick に1回

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
- 初期角速度ベクトル
- 境界活動に対する係数（剛性、変形しやすさ）

### 6.5 初期標高と境界地形の適用

従来の境界モデルを使って初期標高を作るが、時間発展版では以下を守る。

- 過度に完成し切った地形にしない
- 後続Tickで成長・侵食できる余地を残す
- 境界由来の地形は「初期オフセット」として記録可能にする

### 6.6 初期侵食・河川

初回の河川計算と簡易侵食は任意とする。

## 7. Tick更新フェーズ

### 7.1 更新順序（1回の地形更新）

1. マントル熱場の更新
2. プレート運動更新
3. 地殻属性の移流
4. 境界通過処理による離散属性更新
5. 境界再抽出/再分類
6. 構造起伏変化の適用
7. 海洋熱沈降の適用
8. 侵食・堆積の増分を地形へ反映（thickness含む）
9. アイソスタシー調整
10. 活動量メトリクス更新

注:
- 8は実装上、`Hydrology` ステージで算出された `erosion_rate` / `deposition_rate` を
  `Hydrology` ステージ直後に地形へ反映する処理に対応する。
- 海陸比を固定目標へ寄せる補正は、この更新順序には含めない。

### 7.2 プレート運動更新

各プレートは球面上の剛体回転として扱う。

- `omega[plate]` を角速度ベクトルとする
- 頂点位置そのものは共有メッシュを固定し、まずは「速度場と境界判定」だけを更新してよい
- 境界移動を本格導入する場合は、後述の `plate_id` 再割当モードを使う

### 7.3 地殻属性の移流

地殻属性は以下の2系統に分けて扱う。

- 離散属性: `plate_id`, `crust_type`

  - 境界通過で切り替える
- 連続属性: `age`, `thickness`, `density`

  * MUSCL系の移流で更新する

この分離により、境界ぼけを抑えつつ連続量の移動を表現する。

### 7.4 構造起伏変化

構造起伏変化は、境界応力と火山活動に由来する短中期の標高変化である。

```text
tectonic_uplift
= compressive_stress * tectonic_uplift_gain
+ volcanism * volcanic_uplift_gain
```

```text
tectonic_subsidence
= tensile_stress * tectonic_subsidence_gain
```

火山による厚化は、標高だけでなく地殻厚にも加える。

```text
thickness[cell] += volcanism * volcanic_thickening_gain
```

### 7.5 海洋熱沈降

海洋熱沈降は、海洋地殻の冷却と高密度化に伴う長期変化として、構造沈降とは独立に扱う。

```text
if crust_type[cell] == Oceanic {
    age_norm = clamp(age[cell] / age_ref, 0, 1)
    thermal_subsidence = thermal_subsidence_gain * thermal_curve(age_norm)
} else {
    thermal_subsidence = 0
}
```

`thermal_curve` は単調増加関数とし、初版では `sqrt(age_norm)` または同等の緩やかな曲線でよい。
海洋地殻の密度更新はこの熱沈降項と整合するように設計するが、密度増加と沈降量を同一式に直結させない。

### 7.6 アイソスタシー調整

アイソスタシー調整の平衡高度は地殻厚と密度から求める。

```text
h_eq = thickness * (1 - density_c / mantle_density)
```

更新は緩和型とする。

```text
isostatic_adjustment = (h_eq - height[cell]) * isostatic_adjustment_rate
height[cell] += isostatic_adjustment
```

公開用 `vertex_buoyancy` は `h_eq - height` として保持する。

アイソスタシー調整は地形変化の主因ではなく、厚さと密度に対する平衡追従として扱う。

### 7.7 侵食・堆積との連動

侵食と堆積は `height` だけでなく `thickness` にも反映する。

```text
height[cell] -= eroded
thickness[cell] -= eroded * erosion_thickness_coupling
```

```text
height[cell] += deposited
thickness[cell] += deposited * deposition_thickness_coupling
```

これにより侵食後のリバウンドと堆積盆の沈降を表現する。
ただし、河道・湖沼・デルタの形成判定と侵食量・堆積量の算出責務は引き続き `Hydrology` に置く。

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
- `GeologyOutput` のみを保存して地形内部状態を再構築する運用は不可とする

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
