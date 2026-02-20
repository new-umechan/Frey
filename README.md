# Frey
100days企画: day020

世界を作る。
非同期グラフオートマトンで、大陸から文明までをモデル化する。


## 構成
地形・気候生成
歴史生成

自然条件の与える社会構造の規定を

## 取る立場
環境決定論か環境可能論か
→ 環境決定論の立場をとるが、乱数ももちろん入れる。

## 技術スタック
Web+WASM
言語: Javascript, Rust
WebGPU

## データ構造
頂点それぞれをセルとして、情報を保持させる

- 中心の頂点座標のリスト
- 近傍セルの可変長リスト
- (描画用)三角形描画用

```rust
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub struct Mesh {
    // 1) Geometry（描画・計算の基準）
    pub base_pos: Vec<Vec3>,      // len = V（unit sphere）
    pub tri_indices: Vec<u32>,    // len = 3*F（三角形描画用）

    // 2) Topology（近傍：CSR）
    pub nbr_offsets: Vec<u32>,    // len = V+1
    pub nbrs: Vec<u32>,           // len ≈ 6V（5/6近傍が入る）

    // 3) State（セルの属性）
    pub height: Vec<f32>,         // len = V
    pub temp: Vec<f32>,           // len = V
    pub humid: Vec<f32>,          // len = V
    pub plate_id: Vec<u32>,       // len = V
    pub pop: Vec<f32>,            // len = V
    pub state_id: Vec<u32>,       // len = V
}
```


## 処理
ざっくりと
MESH生成→地形生成→気候生成→歴史生成

### MESH
正二十面体
再帰分割 L=6
分割したそれぞれの点をボロノイ分割でセルができる。
（仮想上のもので、実装はしない）
正二十面体の特徴上、12個だけ五角形ができる

### 地形生成
入力: seed値
出力: 海岸線、標高、気候、川
デフォルトはseed値が"earth"で、
この世界を作る。
seed値に別のものを入力すると

1. プレート作成→動かす


海洋プレートは標高ベースを低く
大陸プレートは高く

2. 川の作成、侵食



### 具体的な入出力形式
入力: 文字列をseed値として、それをhashするかたちにする

## 気候生成


## 歴史生成
入力: 海岸線、標高、気候、川
出力：画面左に世界が出て、画面右にlogが出る。
戻したり送ったりできるようにしたい

### 画面
左：マップ（人口密度、国家など複数のビューを切り替えられる）
また、地球儀にも対応
右：イベントログ
	クリックでその時点でジャンプ

## Day020 実装状況 (MESH表示まで)

Rust(WASM)で正二十面体の再帰分割メッシュを生成し、Webでワイヤーフレーム表示するところまで実装済み。

- メッシュ生成: Rust + wasm-bindgen
- 分割レベル: `L=6`
- 描画: Three.js (ワイヤーフレーム)
- 操作: 回転・ズーム対応

### 実行手順

前提:
- Node.js 20+
- Rust stable
- `wasm-pack` インストール済み

```bash
npm install
npm run dev
```

`npm run dev` は内部で `wasm-pack build rust --target web --out-dir ../src/wasm --release` を実行後、Vite開発サーバーを起動する。

### 主要ファイル

- `rust/src/lib.rs`: icosphere生成 (`generate_mesh(level)`)
- `src/main.js`: WASM呼び出しとThree.js表示
- `src/style.css`: ビューワースタイル
- `index.html`: エントリーポイント
