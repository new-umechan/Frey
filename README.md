# Frey

100days企画: day020-

世界を作る。
非同期グラフオートマトンで、大陸から文明までをモデル化する。

## Design Philosophy

- 入力は文字列seedとパラメータ群
- 同じseedとパラメータなら同じ世界を再現できる決定性を重視
- 基本立場は環境決定論。ただし、生成時乱数を導入


## Docs

データモデル、処理の流れ: `docs/architecture.md`
プレート地形生成の仕様:   `docs/plate_spec.md`

## Teck Stack
- Web + WASM
- Rust（計算コア）
- JavaScript（レンダリングとUI）
- Three.js（現状の描画）

## 現在の実装状況

Day020時点では、Rust(WASM)で正二十面体の再帰分割メッシュを生成し、Webでワイヤーフレーム表示するところまで実装済み。

- メッシュ生成: Rust + wasm-bindgen
- 分割レベル: L=6
- 描画: Three.js (ワイヤーフレーム)
- 操作: 回転・ズーム対応
