# Persistent material 境界反応の内部 substep

## Status

Rejected

## Context

Crust 期の公開 tick は 500 万年であり、level 6 では plate の剛体移動が 1 tick に
1 cell 幅を超えることがある。persistent material element の剛体回転はこの変位を直接扱えるが、
海嶺生成と沈み込み除去は移流後の局所 cell band で反応する。そのため、1 update の移動が
cell 幅を超えると、通過した境界を反応が十分に解像できず、gap / overlap が累積する。

`seed=alpha`、level 6 の現行実装では tick 57 に
`max_cell_crossing_fraction_per_tick=1.400`、`persistent_material_gap_ratio=0.158`、
`persistent_material_overlap_ratio=0.448` を観測した。この coverage ambiguity を marker からの
cell label 再構成が埋めるため、plate view に大きな ownership churn と入り組んだ境界が現れる。

## Proposal

公開 tick の plate 変位は変えず、persistent material の移流・投影・境界反応だけを内部 substep に分ける。
1 substep の最大 Euler 変位が level ごとの平均 mesh edge 幅の 0.9 倍以下になるように substep 数を決め、
各 plate の角速度をその数で割る。各 substep では persistent element をそのまま次へ渡し、cell projection から
element を再構成しない。

これは物理速度を制限する変更ではなく、境界反応の時間積分を固定 mesh の空間解像度に合わせる数値近似である。
移流項を空間・時間刻みに対して解像する考え方は Courant, Friedrichs and Lewy (1928) の安定条件に基づく。
persistent surface の材料界面表現は、既存どおり面積と一次モーメントを用いる
Dyadechko and Shashkov (2008) の Moment-of-Fluid 再構成を局所反応に使う。

- Courant, R., Friedrichs, K. and Lewy, H. (1928), _Über die partiellen Differenzengleichungen
  der mathematischen Physik_, Mathematische Annalen 100, 32–74,
  doi:10.1007/BF01448839.
- Dyadechko, V. and Shashkov, M. (2008), _Reconstruction of multi-material interfaces from
  moment data_, Journal of Computational Physics 227(11), 5361–5384,
  doi:10.1016/j.jcp.2007.12.029.

## Validation

- `seed=alpha`、level 6、tick 57 で gap / overlap、ownership churn、orphan、block 指標を現行値と比較する。
- tick 120 の temporary plate-shape gate を通す。
- persistent material の単体テストと geology の統合テストを通す。
- 公開 tick あたりの総 Euler 変位が substep 前と一致することを単体テストで確認する。
- level 6 の実行時間増加を記録し、precompute 運用上許容できるか確認する。

## Trade-off

投影と MoF 境界反応の回数が substep 数に比例して増える。これは性能近似ではなく精度を優先する選択だが、
0.9 cell という上限は厳密な物理定数ではない。shape と coverage の改善が小さい場合、または公開用
precompute の時間が許容できない場合は採用せず、共有 boundary interface の永続化を再検討する。

## Outcome

`seed=alpha`、level 6、tick 60まで比較して棄却した。tick 57 の
`persistent_material_overlap_ratio` は 0.4480 から 0.4378 へわずかに減ったが、
`persistent_material_gap_ratio` は 0.1581 から 0.1621、`orphan_cell_count` は 7 から 9 へ悪化した。
60 tick の実行時間も約28秒から約41秒へ増えた。局所反応の時間解像度だけでは、独立した
剛体 plate surface の大域的な gap / overlap を閉じられない。

## Rejected distinction

`260714-substepped-geometric-remap.md` で棄却した方式は、各 substep で dominant cell label から
material geometry を再構成し、subcell 移動を失って ownership を凍結した。本案は persistent element を
再構成しないため、fractional motion は全 substep を通じて蓄積する。
