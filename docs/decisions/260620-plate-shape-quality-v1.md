# Plate shape quality v1

## Status

Accepted

## Context

plate 数を major plate 相当の `5-7` へ安定化できても、
plate 形状自体が入り組みすぎると Earth-like な major plate に見えにくい。

現在の damage-first emergence は、

- `valid_count`
- `largest_ratio`
- `tiny_fragment_ratio`

を主に見ており、最終的に得られた各 plate の shape quality は直接見ていない。

## Decision

emergence 候補ごとに final `plate_id` を一度組み立て、plate shape 指標を測る。
v1 では次を使う。

- `single_cell_plate_count`
- `min_plate_cells`
- `mean_plate_boundary_complexity`
- `max_plate_boundary_complexity`

`plate_boundary_complexity` は、plate の inter-plate neighbor contact 数を
`sqrt(cell_count)` で割った proxy とする。

目的は厳密な球面幾何の perimeter 復元ではなく、
「cell 数の割に境界が長すぎる plate」を検出することにある。

`mobile_lid` 候補の score には次の penalty を足す。

- singleton / degenerate plate
- boundary complexity が高すぎる plate

また、boundary extraction の threshold sweep は狭い固定帯に閉じず、
damage から作る boundary mask の保持率を広めに探索する。
これは score を恣意的にいじるためではなく、
seed ごとに異なる damage localization から複数の boundary band 幅を試すためである。

## Consequences

利点:

- plate 数だけでは見えない「入り組みすぎ」を候補選択へ反映できる
- visual inspection と diagnostics を同じ指標で繋げられる
- narrow な sweep では出なかった split 候補を拾える余地が増える

欠点:

- complexity proxy は cell-count ベースの近似であり、厳密 perimeter ではない
- boundary band の太さや mesh level に依存するので、将来 skeleton 導入後に見直し余地がある
- 候補数が増えるぶん emergence 診断と初期化コストは少し増える
