# エントリーポイント設計

## main.jsの責務

`src/main.js`は次の3つだけを担う。

1. アプリケーション起動
2. メインループ実行
3. モジュールの接着（`createApp`の呼び出し）

シミュレーション計算、UIイベント処理、描画の詳細ロジックは持たない。

## 実装配置

- エントリ: `src/main.js`
- アプリ統合: `src/app/app.js`
- UI操作: `src/ui/controls.js`
- カメラ/表示面切替: `src/gfx/views/camera-controller.js`
