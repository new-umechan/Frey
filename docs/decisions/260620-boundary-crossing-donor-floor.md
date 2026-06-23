# Boundary crossing donor floor

## Status

Accepted

## Context

Crust runtime の ownership transfer では、boundary activity に応じて
隣接 plate へ `plate_id` を移す。

`epsilon` seed の追跡で、initial emergence は 7 plate を作れていても、
runtime 中に一つの plate が `5 -> 3 -> 1` cell と縮退し、
最終的に吸収されて `7 -> 6` へ落ちることが分かった。

これは plate 数 target の問題ではなく、boundary crossing が
degenerate な micro-plate を作れてしまう問題である。

## Decision

Boundary crossing では、donor plate が 3 cell 以下へ縮退する reassignment を許さない。

- 判定は runtime の `plate_id` cell count で行う
- guard は donor 側にだけ掛ける
- 1 cell / 2 cell の degenerate block を新たに作らないことを優先する

目的は plate 数を直接維持することではなく、
剛体 block と呼びにくい極小 plate の生成を抑えることにある。

## Consequences

利点:

- emergence で得た plate count が、runtime の局所的な ownership transfer だけで
  直ちに崩れる退行を抑えられる
- `single_cell_plate_count` と `plate_id_churn_rate` の解釈が自然になる

欠点:

- 極小 plate の消滅が遅くなる seed はありうる
- 3 cell という floor 自体は近似であり、mesh level 依存の見直し余地がある
