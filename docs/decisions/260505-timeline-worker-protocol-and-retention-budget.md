# Timeline Worker Protocol / Retention Budget の導入

## Status

Accepted

## Context

timeline runtime の公開面は整ってきたが、
worker protocol と UI 側の操作体系がまだ旧 API 名に引きずられている。

また retention policy は件数制限のみで、
undo log / checkpoint のメモリ使用量を runtime から観測できない。

## Decision

- worker / client の正本 request 名を timeline 語彙へ寄せる
- `TimelineRetentionPolicy` に `max_estimated_bytes` を追加する
- `TimelineRuntime` は checkpoint / undo log の推定使用量を計算し、prune に使う
- `TimelineStateResponse` に retention と estimated usage を含める

## Consequences

利点:

- UI / worker が timeline cursor と branch を直接扱いやすくなる
- retention の運用方針が件数だけでなくメモリ予算でも見える
- `get_timeline_state` がデバッグと将来の HUD に使いやすくなる

コスト:

- 推定サイズ計算の保守が必要になる
- worker protocol とテストの更新が必要になる
