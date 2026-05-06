# Checkpoint Pruning Preserves Seek Value

## Status

Superseded by `../decisions/260506-checkpoint-pruning-preserves-seek-value.md`

## 背景

retention では初期 checkpoint と最新 checkpoint を守っているが、
`checkpoint_limit` 超過時の prune はまだ単純 oldest である。

この方式だと中間の再生足場が uneven になりやすく、
同じ件数でも seek 時の replay 距離が偏る。

## 提案

- checkpoint prune は「最も冗長な中間 checkpoint」から落とす
- 少なくとも次は優先保持する
    - 初期 checkpoint
    - 最新 checkpoint
    - current tick に最も近い checkpoint
- 中間候補は、両隣 gap が最も小さいものから prune する

## 成功条件

- `checkpoint_limit` 超過時に oldest 一辺倒で減らさない
- seek の足場が timeline 上で過度に偏らない
- `application::world_` テストと build が通る
