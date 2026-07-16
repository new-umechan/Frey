# Plate block budget and lifecycle

## Status

Draft

## Context

`plate_id` は現在、固定数の剛体 plate として扱っている。
しかし runtime では同じ `plate_id` が細い neck で複数塊に分かれたり、
別 plate と一体化しうる。
world/global budget や plate 単位 budget だけでは、この局所的な block の独立性を表せない。

## Proposal

将来は `plate_id` の内部に `plate block` を導入する。
block は陸塊ではなく、同じ `plate_id` の robust core component とする。

段階的に進める。

1. block diagnostics を追加し、multi-block 化を測る
2. ownership transfer budget を block 単位へ配分する
3. persistent な block 分離/同期から split/merge candidate を作る

## Notes

初期実装では lifecycle は行わない。
`plate_block_count`、`secondary_plate_block_ratio` などを validation として観測する。
`weak_line_plate_block_count` は、保存済みの boundary activity と runtime stress を
弱線 proxy として使う診断である。
budget 配分を変える場合は、local front や block へ配分したうえで、
micro-fragment が増えないことを先に gate する。

## Close when

split/merge lifecycle を実装する場合は `Accepted`、固定 plate identity を維持する場合は
`Rejected` にする。
