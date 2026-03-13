# Frey

100days企画: day020-

地形や気候などの地理的な制約から、国家・戦争・言語圏の興亡までを因果的に生成する歴史シミュレータ

## Design Philosophy

- 入力は文字列seed
- 神（ユーザー）の手を途中で加えられるように。
  介入logは保存しておく。
- 同じseedとパラメータなら、だいたい同じ世界を再現。
  ある程度の揺らぎは許容するが、マクロな構造（大陸配置・河川系・文明分布）は再現したい
- 基本立場は環境決定論。ただし生成時は制御された疑似乱数を導入

## Docs

docs/README.mdに仕様の全体像をメモ

## Teck Stack

- Web + WASM
- Rust: 計算コア
- Vite: 開発サーバー
- JavaScript（レンダリングとUI）
- Three.js（現状の描画）

## Development

`npm run dev`で開発サバーを起動できる。
開発中に`rust/`を編集するとWASMを自動で再ビルドしてVite画面に反映される。
`config/terrain.yaml`編集時は地形パラメータを同期し、必要な再ビルドが走る。
`config/runtime.yaml`編集時はランタイム制御パラメータを同期し、Vite画面へ反映される。
