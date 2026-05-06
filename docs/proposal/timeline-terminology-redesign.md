# タイムライン用語再設計

## Status

Accepted

## 背景

現行の時間操作まわりでは、`history`、`restore`、`world delta` という語が混在している。

- `history` は checkpoint 一覧と時間軸全体の両方を指しうる
- `restore` は seek と同義だが、branch や cursor という将来概念とつながりにくい
- `delta` は表示差分と逆再生用の巻き戻し記録の両方を連想させる

この曖昧さは、逆再生前提の runtime 再設計を進める際の障害になる。

## 目的

- 将来の逆再生・seek・branch 設計に耐える用語へ整理する
- transport 向け差分と内部の巻き戻し記録を別語に分離する
- 公開 API と内部型の正本名を先に揃え、後続の実装変更をやりやすくする

## 提案概要

次を新しい正本語とする。

- 時間軸全体: `timeline`
- seek 基点の保存点: `checkpoint`
- 表示更新用の差分: `view delta`
- 1 tick の変更集合: `tick change set`
- 巻き戻し用の変更前値記録: `tick undo log`

旧語の扱いは次の通りとする。

- `history` は checkpoint 一覧の旧 API 名にだけ残す
- `restore` は旧 API 名にだけ残し、正本は `seek` とする
- `world delta` は旧 API 名にだけ残し、正本は `view delta` とする

## スコープ

- runtime / application / WASM API の型名と use case 名
- `docs/reference/interface/wasm_api.md`
- `docs/reference/architecture/data_model.md`
- `docs/concepts/runtime_layers.md`

今回は次をスコープ外とする。

- 実際の `TickUndoLog` 実装
- `TimelineRuntime` の本格導入
- 既存 UI 側の全面 rename

## 成功条件

- 新規実装は `timeline` / `checkpoint` / `view delta` を正本語として参照する
- 既存の公開 API は互換 alias として残る
- 文書上で `delta` が巻き戻し記録を意味しないことが明示される

## リスクとトレードオフ

- 互換 alias を残すぶん、一定期間は新旧両名が共存する
- 型 alias が残るため、完全移行までは grep のノイズが増える

ただし、ここで命名を正さないと、後続の逆再生設計で `delta` と `history` の意味がさらに曖昧になる。

## 実施計画

1. proposal と decision で新用語を固定する
2. reference docs を新用語へ更新する
3. Rust の DTO / runtime / use case / WASM binding に新名を導入する
4. 旧名は互換 alias と wrapper として残す
5. テストを新名ベースへ更新し、旧名 alias も最低限検証する

## 未解決事項

- `TimelineArchive` を将来 `TimelineStore` へさらに改名するか
- `WorldTransportCache` を `TimelineViewCache` へ全面移行するタイミング
- `fork_world` を `fork_timeline_branch` に置き換える公開スケジュール
