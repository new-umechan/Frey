# Seek が Head Tick を超えても成立する単一 Timeline

## Status

Accepted

## Context

単一 timeline モデルでは、`seek` は既存履歴内の移動だけでなく、
必要なら未計算領域まで cursor を進められる方が自然である。

## Decision

- `seek_world_to_tick` は `head_tick` を超える target を許可する
- `target_tick <= head_tick` では既存 timeline を再利用する
- `target_tick > head_tick` では `head_tick` まで既存 timeline を再利用し、
  その先だけ新規 tick を計算する
- tick 実行ループは helper に寄せて `advance` と `seek` で共有する

## Consequences

利点:

- `seek` が timeline cursor の正本 API として自然になる
- `advance` と `seek` の内部経路差が減る

コスト:

- `seek` の責務が増えるため、tests で `head_tick` 再利用挙動を固定する必要がある
