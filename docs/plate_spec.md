# プレートに基づく地形生成仕様

## 1. 目的

入力seedとparamsから、プレート運動に整合する初期地形を生成する。
出力は、少なくとも `height`, `plate_id`, `river_flux`, `river_next`を含む。

## 2. 入出力

### 2.1 入力

- `seed: String`
- `params: TerrainParams`

```rust
pub struct TerrainParams {
    pub level: u32,                   // icosphere 分割レベル（推奨 6）
    pub l_max: u32,                   // 球面調和次数（既定 4）
    pub alpha: f32,                   // スペクトル減衰（既定 1.5）
    pub num_plates_min: u32,          // 最小プレート数（既定 8）
    pub num_plates_max: u32,          // 最大プレート数（既定 18）
    pub ocean_plate_ratio: f32,       // 海洋プレート比率（既定 0.65）
    pub boundary_band: f32,           // 境界帯域の閾値（既定 0.08）
    pub uplift_gain: f32,             // 収束境界の隆起係数（既定 0.45）
    pub subduct_gain: f32,            // 沈み込み係数（既定 0.35）
    pub divergent_gain: f32,          // 発散境界の沈降係数（既定 0.20）
    pub smooth_iter: u32,             // 平滑化反復（既定 6）
    pub smooth_lambda: f32,           // 平滑化係数（既定 0.35）
    pub river_rain_base: f32,         // 降水ベース（既定 0.5）
    pub river_accum_threshold: f32,   // 河川成立閾値（既定 0.015）
}
```

### 2.2 出力

```rust
pub struct TerrainOutput {
    pub height: Vec<f32>,         // len = V, [-1, 1]
    pub plate_id: Vec<u32>,       // len = V, [0, num_plates)
    pub river_flux: Vec<f32>,     // len = V, [0, 1]
    pub river_next: Vec<i32>,     // len = V, -1 は海/終端
}
```

seedがearthの場合は生成せず、事前に定義したプリセットを返す。

## 3. 決定性ルール

- 乱数生成器は`StdRng`相当の固定アルゴリズムを採用
- 初期化シードは `SHA-256(seed + canonical_json(params_except_level))` の先頭 16 byte を使用
- 同一入力で bit-level 一致を目標にする（少なくとも `plate_id` 一致、`height` は誤差 `1e-6` 以内）

## 4. 生成手順

### 4.1 メッシュ準備

1. `generate_icosphere(level)` で `base_pos`, `tri_indices` を取得
2. `tri_indices` から無向辺を列挙し、近傍 CSR (`nbr_offsets`, `nbrs`) を構築
3. 各頂点の球面座標 `(theta, lambda)` を計算

### 4.2 マントル場 φ の構築

以下で各頂点 `v` の `phi[v]` を評価する。

```text
phi(theta, lambda) = Σ_{l=2..L_max} Σ_{m=-l..l} c_lm * Y_lm(theta, lambda)
c_lm ~ Normal(0, sigma_l), sigma_l = 1 / l^alpha
```

- 実装は実数球面調和基底（real SH）を使用
- `phi` は評価後に z-score 正規化（平均 0, 分散 1）

### 4.3 プレート種（seed）抽出

1. `phi` の局所極大・極小を検出（全近傍より厳密に大/小）
2. 極大候補を上位 `k_up`、極小候補を下位 `k_down` として採用
3. `num_plates` は `[num_plates_min, num_plates_max]` 内で `seed` 由来に決定
4. `k_up + k_down = num_plates` となるよう比率配分（既定は 1:1）

### 4.4 プレート分割（watershed）

1. 各 seed から多源 BFS でラベル伝播し `plate_id` を決定
2. 伝播コストは `cost = edge_len * (1 + boundary_penalty)`
3. `boundary_penalty = clamp(abs(phi_mid) / boundary_band, 0, 1)` とし、ゼロ交差付近を優先的に境界化
4. 未到達頂点は最短 seed に再割当

### 4.5 プレート属性付与

各 plate `p` に以下を設定。

- `is_ocean`: `rng < ocean_plate_ratio`
- `velocity`: 接平面上の2次元ベクトル（方位角 `dir ~ U(0, 2π)`, 速度 `speed ~ U(0.3, 1.0)`）
- `base_height`:
    - 海洋: `-0.45 ± 0.08`
    - 大陸: `+0.18 ± 0.10`

### 4.6 初期標高

`height[v] = plate.base_height + 0.10 * phi[v] + small_noise(v)`

- `small_noise` は `[-0.03, +0.03]`
- ここまでで `height` を `[-1.2, 1.2]` に一旦 clamp

### 4.7 境界相互作用（隆起/沈降）

異なる `plate_id` を持つ隣接頂点ペア `(i, j)` を境界辺とする。

1. 境界法線方向の相対速度 `v_rel_n` を計算
2. タイプ分類:
    - `v_rel_n > +eps`: 収束
    - `v_rel_n < -eps`: 発散
    - それ以外: 横ずれ
3. 標高補正:
    - 収束:
        - 海洋 vs 大陸: 海洋側に `-subduct_gain`, 大陸側に `+uplift_gain`
        - 大陸 vs 大陸: 両側に `+0.7 * uplift_gain`
        - 海洋 vs 海洋: 片側 `-0.7 * subduct_gain`, 他側 `+0.3 * uplift_gain`
    - 発散:
        - 両側に `-divergent_gain`、中心線に `+0.15 * divergent_gain`（海嶺補正）
    - 横ずれ:
        - 補正なし
4. 補正量は境界距離減衰 `exp(-d^2 / (2 * sigma^2))` で幅 `sigma=2 hop` まで拡散

### 4.8 平滑化

Laplacian smoothing を `smooth_iter` 回適用。

```text
h_new[v] = h[v] + smooth_lambda * (mean(h[nbr(v)]) - h[v])
```

- 各反復後に `[-1, 1]` へ clamp
- プレート境界上は `smooth_lambda * 0.6` に低減し、地形コントラストを維持

### 4.9 海陸確定

- `sea_level = 0.0`
- `height[v] <= sea_level` を海とする
- 海岸線は「海セルと陸セルをまたぐ辺」の集合

### 4.10 河川生成

1. `rain[v] = river_rain_base * max(0, 1 - abs(lat[v]) / (pi / 2))`
2. 流下先 `river_next[v]` は近傍のうち最急降下先（存在しなければ `-1`）
3. 高地から低地順（`height` 降順）で `river_flux` を集水
4. `river_flux[v] < river_accum_threshold` のセルは河川として非表示扱い（値は保持）
5. 海セルは `river_next = -1`

## 5. 計算量目安（L=6）

- 頂点数 `V = 40962`
- 辺数 `E ≈ 3V`
- 主要工程:
    - SH評価: O(V * L_max^2)
    - ラベル伝播: O(E log V)（priority queue 実装時）
    - 平滑化: O(smooth_iter * E)
    - 河川集水: O(V log V)（ソートあり）

## 6. バリデーション基準

### 6.1 形状/範囲

height.len == plate_id.len == river_flux.len == river_next.len == V
height ∈ [-1, 1]
plate_id < num_plates
river_next == -1 or (0 <= river_next < V)

### 6.2 統計

海セル比率: `0.55 .. 0.80`
プレート数: `num_plates_min .. num_plates_max`
プレート最大連結成分が全球の 50% 未満

### 6.3 決定性

同一 `seed + params` で `plate_id` 完全一致
`height` 平均絶対差が `1e-6` 以下

## 7. 失敗時の扱い

seed 抽出不足（極値不足）の場合は farthest-point 補完で plate seed を追加
分割結果に空プレートが出た場合は再ラベル（最小プレートに吸収）
河川ループ検出時は最小勾配辺を切断して `-1` 終端

## 8. 実装優先順

1. `phi` 評価 + `plate_id` 分割
2. 初期 `height` + 境界補正 + 平滑化
3. 海陸確定 + 河川
4. `earth` プリセット分岐
