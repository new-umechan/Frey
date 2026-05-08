# Alpha Era Snapshot Bootstrap

## Status

Accepted

## 背景

`seed=alpha` の日常検証は `Crust` からの積み上げが支配的になり、最新モジュールの変更確認までの待ち時間が長い。
開発時の体感待機を短縮しつつ、公開 API の意味や simulation の意味を変えない導線が必要である。

## 目的

- `alpha` 専用の開発時 bootstrap を短縮する
- 既存公開 API を壊さず、dev 専用 opt-in に限定する
- snapshot 不在・破損・不整合時は安全に通常計算へフォールバックする

## 提案

- era 境界 (`Environment=800`, `Life=1300`, `Civilization=1395`, `History=1445`) ごとに snapshot を生成する
- 正本 cache は `./.cache/frey/alpha-snapshots/` に保存する
- browser 読込用 mirror は `web/public/.dev-precomputed/alpha/` に同期する
- manifest JSON と stage 別 binary を artifact interface とする
- 復元条件は `seed=alpha` かつ dev 指定ありの場合のみ有効化する
    - 環境変数: `FREY_DEV_SNAPSHOT_STAGE`
    - query param: `devSnapshotStage`
- snapshot 不在・破損・fingerprint 不一致時は warning を出して通常経路へ戻る

## 成功条件

- `alpha` の dev 検証で stage 復元が機能し、通常より短時間で最新モジュール検証に入れる
- `seed != alpha` では既存挙動のまま
- snapshot 失敗時に異常終了せず従来計算に戻る
- public API (`init_world(seed, mesh_level, config)`) の互換は維持される

## スコープ

- Rust: snapshot envelope/manifest 型、serialize/deserialize、復元 helper、generator コマンド
- native runner / wasm runner / worker の dev opt-in 組み込み
- docs: proposal/decision/architecture/operations

## スコープ外

- `alpha` 以外の seed 最適化
- 任意 tick への一般化 snapshot
- 本番運用 artifact としての配布フロー拡張

## 再生成トリガー

- snapshot format version 更新
- `GeologyParams` デフォルト変更
- era 境界変更
- 復元対象 state schema 変更
- `alpha` 生成ロジック変更
