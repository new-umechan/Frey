# UI Specification

本書は reference 文書である。現在の UI が満たすべき責務と、将来追加したい操作面をまとめる。

詳細な画面デザインは Figma を参照し、本書では機能境界だけを扱う。

## UI の方針

- 目的はシミュレーションの観測と比較であり、ゲーム風の演出を主目的にしない
- 情報密度は高く保ちつつ、再生制御と観測対象の切り替えを迷わず行えることを優先する
- 見た目の整合よりも、どの値を見ているかが明確であることを優先する

## 主要領域

### Playback

- 再生
- 停止
- 巻き戻し
- 履歴 tick の移動

### Visualization

- 地形、気候、水文などのレイヤ表示
- カメラ操作
- レイヤごとの凡例表示
- 因果探索 Demo Slice の発光点、trace、短い数値ラベル表示

### Inspection

- セルや領域の値を確認するための情報表示
- 主要イベントや変化点のログ表示
- ログから該当 tick へ移動できる導線

### Data I/O

- 書き出し
- 読み込み
- 他環境で生成された世界の再生専用閲覧

## 状態

- 現時点では、詳細レイアウトよりも機能責務の整理を優先する
- UI 実装の公開インターフェースは `docs/reference/interface/wasm_api.md` に依存する

## 因果探索 Demo Slice

- この節は恒久 UI 仕様ではなく、体験検証用の実験境界を記録する
- 初回は `border_mountain_plate_demo` を既存 Three.js scene 上の追加レイヤとして表示する
- 3 feature と 3 trace だけを描画し、UI で trace を増やさない
- `hover/focus` は近接反応として feature の発光と短い数値ラベルを強める
- `click/tap` で trace を固定し、選択中 trace の evidence 種別と不確実性理由だけを短く出す
- `trace click/tap` は target feature へフォーカスを移し、次の探索対象を強調する
- 色、太さ、流速、揺らぎは `display_mapping` からだけ導く
- 国境と山脈の関係は直接因果として説明しない
