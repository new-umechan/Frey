# Branch を持たない単一 Timeline Runtime

## Status

Accepted

## Context

逆再生前提の runtime として `TimelineRuntime` を置く方針は維持するが、
この段階では timeline 分岐や複数 future を扱わない。

必要なのは単一 timeline 上での `advance / rewind / seek` であり、
UI も変更しない。
この前提では branch metadata と fork API は責務過多になる。

## Decision

- timeline は単一時間軸として扱う
- `TimelineRuntime` は `current_tick` と `head_tick` を持つ
- branch 系 metadata と fork API は正式設計から外す
- `rewind` / `seek` は cursor 移動であり、未来側履歴を破棄しない
- `advance_timeline` は `head_tick` より先に出たぶんだけ新規 tick を計算する
- `checkpoint` は長距離移動の高速化、`TickUndoLog` は短距離巻き戻しの高速化に使う
- `tick 完了境界` を時間操作の公開整合点とする

## Consequences

利点:

- runtime の責務が「単一 timeline の移動」に絞られる
- `rewind` / `seek` / `advance` の意味が単純になる
- UI を変えずに application/runtime/worker だけ再編できる

コスト:

- fork や複数 timeline を使う用途は一旦サポートしない
- `seek` 時に `head_tick` までの再利用戦略を慎重に実装する必要がある

## Notes

この decision は決定性を強く契約化するものではない。
まずは branch なし単一 timeline モデルを正本にし、
将来必要になった時点で intervention や複数 future の扱いを再検討する。
