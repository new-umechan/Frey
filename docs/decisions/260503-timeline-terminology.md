# タイムライン用語の正本化

## Status

Accepted

## Context

逆再生前提の runtime 設計を進めるにあたり、現行の `history`、`restore`、`world delta` は意味が広すぎる。

- `history` は時間軸全体と checkpoint 一覧を混同しやすい
- `restore` は cursor 移動よりも復元処理を連想しやすい
- `delta` は表示差分と巻き戻し記録を誤って結びつけやすい

このままでは、時間操作の API / 型 / docs を拡張するたびに語義がぶれる。

## Decision

時間操作まわりの正本語を次で固定する。

- 時間軸全体は `timeline`
- 保存点は `checkpoint`
- 表示差分は `view delta`
- 巻き戻し用の過去値記録は `undo log`

命名規則は次の通りとする。

- `Delta` は transport / view 向け差分にだけ使う
- `ChangeSet` はその tick の変更集合に使う
- `UndoLog` は巻き戻し用の過去値記録に使う
- `Snapshot` は完全保存状態にだけ使う

公開 API は新名を正本とし、旧名は互換 alias として残す。

## Consequences

利点:

- 今後の `seek` / `rewind` / `branch` 設計で用語がぶれにくくなる
- 表示差分と巻き戻し記録を誤って同一視しにくくなる
- docs とコードの正本が時間軸中心の語彙に寄る

コスト:

- 移行期間は新旧 API 名が並存する
- 型 alias と wrapper が一時的に増える

## Notes

今回の decision は命名の固定であり、`TickUndoLog` や `TimelineRuntime` の完全実装を意味しない。
それらは別 proposal / decision で扱う。
