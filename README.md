# Frey

100days企画: day020-

世界を作る。
非同期グラフオートマトンで、大陸から文明までをモデル化する。

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
開発中にrust/や、config/配下を編集すると、WASMを自動で再ビルドしてViteの画面に反映される。
