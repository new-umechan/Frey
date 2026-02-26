# Frey

100days企画: day020-

世界を作る。
非同期グラフオートマトンで、大陸から文明までをモデル化する。

## Design Philosophy

- 入力は文字列seedとパラメータ群
- 同じseedとパラメータなら同じ世界を再現できる決定性を重視
- 基本立場は環境決定論。ただし生成時は制御された疑似乱数を導入

## Docs

データモデルと処理の流れはdocs/architecture.md。
プレート地形生成の仕様はdocs/plate_spec.md。
画面についてはdocs/ui_spec.md

## Teck Stack

- Web + WASM
- Rust: 計算コア
- Vite: 開発サーバー
- JavaScript（レンダリングとUI）
- Three.js（現状の描画）

## Development

`npm run dev`で開発サバーを起動できる。
開発中にrust/や、config/配下を編集すると、WASMを自動で再ビルドしてViteの画面に反映される。
