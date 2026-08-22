# Marker ownership core の時間連続な選択

## Status

Rejected

## Context

persistent material の表示用 `plate_id` は、初期 material element の marker が cell center を覆うかで
単一候補 core を作り、曖昧な gap / overlap をその core から埋めて再構成する。同じ plate の単一候補領域が
複数 component になった場合、現行実装は最大 component だけを残す。

独立剛体 plate の overlap が大きい局面では、近い大きさの component の順位が tick 間で入れ替わる。
別の component を ownership seed に選ぶと、その plate の曖昧領域が全球規模で塗り直される。
`seed=alpha`、level 6 では tick 47 に `plate_id_churn_rate=0.333` を観測しており、物理的な
cell crossing だけでは説明できない不連続な raster ownership になっている。

## Proposal

同じ plate の marker core component は、cell 数だけでなく直前 tick の同一 plate cell との重なりを加点して選ぶ。
score は `component_cell_count + 2 * retained_previous_cell_count` とする。直前 ownership を完全に優先せず、
新 component が旧 component の3倍を超えれば交代できる。

これは persistent material の面積、位置、地殻種別、年齢、境界反応を変更しない。固定 mesh に表示用 label を
サンプリングする際の時間 coherence 制約である。材料界面の正本は、既存どおり面積と一次モーメントを保存する
Dyadechko and Shashkov (2008) の Moment-of-Fluid 表現、および persistent element とする。

- Dyadechko, V. and Shashkov, M. (2008), _Reconstruction of multi-material interfaces from
  moment data_, Journal of Computational Physics 227(11), 5361–5384,
  doi:10.1016/j.jcp.2007.12.029.

## Validation

- `seed=alpha`、level 6 の tick 40〜60で churn spike、orphan、component、block 指標を比較する。
- tick 57 の plate shape が改善し、material gap / overlap が不変であることを確認する。
- tick 120 の temporary plate-shape gate と Rust test suite を通す。
- level 6、1600 tick の persistent material projection が完走することは公開 store 更新前に別途確認する。

## Trade-off

曖昧な overlap / gap 内では、面積だけなら別 core を選ぶ局面でも直前 ownership を短期間維持する。
score の係数2は物理定数ではなく、cell raster の離散的な component 交代を安定化する数値パラメータである。
churn だけを下げて細線・分断を悪化させる場合は採用しない。

## Outcome

`seed=alpha`、level 6、tick 60まで比較して棄却した。tick 47 の churn spike は
0.333 から 0.033 へ下がった一方、component の交代を tick 57 まで遅らせただけで、同 tick の churn は
0.036 から 0.123 へ悪化した。tick 57 の orphan も7から9、最大 detached fragment ratio も
0.0031から0.0050へ増えた。単一 core の選択順だけでは不連続な component 交代を解消できない。
