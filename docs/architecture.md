# Architecture

## データモデル

球面上の各頂点を 1 セルとみなし、幾何情報、近傍トポロジ、状態量を保持する。
状態量には標高、温度、湿度、プレートID、人口、国家ID、河川流量、河川の流下先を含む。

```rust
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub struct Mesh {
    pub base_pos: Vec<Vec3>,
    pub tri_indices: Vec<u32>,
    pub nbr_offsets: Vec<u32>,
    pub nbrs: Vec<u32>,
    pub height: Vec<f32>,
    pub temp: Vec<f32>,
    pub humid: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub pop: Vec<f32>,
    pub state_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}
```

## 処理の流れ

1. メッシュ生成
2. 地形生成（プレート、標高、川）
3. 気候生成
4. 歴史生成

## MESH生成仕様

- 基本形状は正二十面体
- 再帰分割レベルはL=6を中心に運用
- 頂点は単位球面へ正規化
- 描画は三角形インデックスを利用

## 地形生成

地形生成は、場の生成、プレート分割、境界補正、平滑化、海面再調整、河川計算の順で進む。
詳細仕様はdocs/plate_spec.mdを参照。

## 描画ポリシー

海面下の地形値は計算結果として保持するが、現行の描画では海面下の頂点変位を反映しない。
海で亀裂のように見えるアーティファクトを避けるため、地形変位は陸地のみへ適用する。

## 設定
地形生成: config/terrain-params.yaml
