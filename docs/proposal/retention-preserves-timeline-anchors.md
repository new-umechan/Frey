# Retention Preserves Timeline Anchors

## Status

Accepted

## 背景

単一 timeline で future を保持する場合、
retention pruning が単純に「一番古いものから削る」だけだと、
長距離 seek や `head_tick` 再利用に必要な checkpoint が消えやすい。

## 提案

- retention は単なる byte / count 制御ではなく、
  timeline の anchor を守る
- 少なくとも次は優先保持する
    - 初期 checkpoint
    - 最新 checkpoint
- prune 順は次を基本にする
    - 古い undo log
    - 中間 checkpoint
- 予算を超えても anchor しか残らない場合は、それ以上 prune しない

## 成功条件

- 極端な retention budget でも timeline が完全に seek 不可能にならない
- `head_tick` 再利用と長距離 replay の最低限の足場が残る
