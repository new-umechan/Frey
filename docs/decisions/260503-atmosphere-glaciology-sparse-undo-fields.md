# climate の残り連続値列と glaciology を sparse undo 対象へ広げる

## Status

Accepted

## Context

second stage で climate / hydrology の一部を sparse 化したが、
climate にはまだ多数の `Vec<f32>` が残っている。
また glaciology は field 全体が連続値列であり、
subsystem 丸ごと copy より sparse patch と相性がよい。

## Decision

third stage の sparse undo 対象として次を追加する。

- climate の残り `Vec<f32>` 全て
- glaciology の `Vec<f32>` 全て

実装方針は以下とする。

- climate は selected field で全 field を網羅する
- glaciology は `GlaciologyUndoState` で全 field を網羅する
- field 長不一致など sparse patch を安全に構築できない場合のみ full copy へフォールバックする

## Consequences

利点:

- climate / glaciology の undo log メモリ量を段階的に削減できる
- `TickUndoLog` が SoA の変更密度に沿った形へ近づく

コスト:

- undo state の field 数と適用分岐は増える
- 将来的には field ごとの helper / macro 化を検討する余地がある
