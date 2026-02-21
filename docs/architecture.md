# Architecture

## データモデル

球面上の各頂点を1セルとみなし、以下の情報を保持する。

```rust
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub struct Mesh {
    // Geometry
    pub base_pos: Vec<Vec3>,      // len = V（unit sphere）
    pub tri_indices: Vec<u32>,    // len = 3 * F（三角形描画用）

    // Topology（CSR）
    pub nbr_offsets: Vec<u32>,    // len = V + 1
    pub nbrs: Vec<u32>,           // len ≈ 6V（5/6近傍）

    // State
    pub height: Vec<f32>,         // len = V, [-1, 1]
    pub temp: Vec<f32>,           // len = V
    pub humid: Vec<f32>,          // len = V
    pub plate_id: Vec<u32>,       // len = V
    pub pop: Vec<f32>,            // len = V
    pub state_id: Vec<u32>,       // len = V
    pub river_flux: Vec<f32>,     // len = V
    pub river_next: Vec<i32>,     // len = V, -1 は終端
}
```

## 処理の流れ

1. MESH生成
2. 地形生成（プレート・標高・川）
3. 気候生成
4. 歴史生成

## MESH 生成仕様

- 基本形状: 正二十面体
- 細分化: 再帰分割 `L=6`
- 頂点は単位球面に正規化
- 描画は三角形インデックスを利用

※ 五角形セルは正二十面体を使うため、12個生まれる

## 地形生成

1. プレートから外形生成
2. 川の侵食
3. 細部処理

プレートの生成は `docs/plate_spec.md` に詳しく仕様をまとめた
