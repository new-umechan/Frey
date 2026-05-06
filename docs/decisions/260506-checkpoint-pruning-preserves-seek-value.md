# 260506 Checkpoint Pruning Preserves Seek Value

## Status

Accepted

## Context

単一 timeline の seek 性能は checkpoint 配置に強く依存する。
初期 / 最新 anchor だけを守っても、中間 checkpoint を oldest 順で消すと
replay 距離の偏りが大きくなりやすい。

## Decision

- checkpoint prune は seek 価値を基準に行う
- 初期 checkpoint / 最新 checkpoint / current tick 最寄り checkpoint を保護する
- prune 候補は中間 checkpoint のうち、両隣との gap が最も小さいものを優先する

## Consequences

- 同じ checkpoint 件数でも seek 用の足場を均しやすくなる
- retention はなお近似方針だが、単純 oldest より replay 距離の悪化を抑えやすい
