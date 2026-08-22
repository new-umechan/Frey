# Plate view の detached dust closure

## Status

Rejected

## Context

persistent material element は plate 物質の正本だが、固定 mesh の `plate_id` には sampling による
1〜数 cell の detached component が残る。`seed=alpha`、level 6、tick 57 では最大7 component、
最大 detached fragment ratio 0.0031、orphan 7 cell を観測した。主要 plate の境界複雑度は Earth の
参照分布と同程度だが、この小断片は plate view で強い色の飛び地となり、形を実際以上に乱して見せる。

simulation の `plate_id` を補正する試行では、補正後の label が次 tick の境界反応へ入力され、tick 80〜100の
細枝指標を悪化させた。表示上の問題を物理 state の mutation で直すべきではない。

## Decision

simulation と precomputed store の `plate_id` は変更しない。plate view 用にだけ `plateDisplayId` を導出する。

1. terrain geometry の triangle index から cell adjacency を一度構築する。
2. plate ごとに最大連結成分を特定する。
3. 最大成分以外のうち4 cell以下の componentだけを display dust として未解決に戻す。
4. 確定済みの隣接 display label の多数決で外側から埋める。
5. 同数なら元と異なる plate、確定 cell 数が多い plate、小さい plate ID の順で決定論的に選ぶ。

5 cell以上の component は物理的な分裂候補と表示 artifact を区別できないため残す。hover、click selection、
selected highlight は描画色と同じ `plateDisplayId` を使う。統計、simulation、field API、保存データは正本の
`plate_id` を使い続ける。

この closure は material interface を変更しない。正本は、既存どおり面積と一次モーメントを保存する
Dyadechko and Shashkov (2008) の Moment-of-Fluid 表現、および persistent element である。

- Dyadechko, V. and Shashkov, M. (2008), _Reconstruction of multi-material interfaces from
  moment data_, Journal of Computational Physics 227(11), 5361–5384,
  doi:10.1016/j.jcp.2007.12.029.

## Validation

- 人工 adjacency で1 cellの detached componentが表示上だけ吸収され、入力が不変であることを確認した。
- 5 cellの detached componentを保持することを確認した。
- triangle index から決定論的な無向 adjacencyを構築することを確認した。
- plate field sync、cell metric と合わせた web test 7件、および web lintを通した。
- simulation code を変更しないため、既存の tick 120 temporary shape gate の正本値は変わらない。

## Trade-off

実際の plate split が4 cell以下で始まる場合、plate view はその初期段階を表示しない。raw `plate_id` の field APIと
plate statsには小断片が残るため、診断値と画面上の component 数は一致しないことがある。将来 plate split と
plate ID lifecycleを実装した場合は、イベント情報を表示側へ渡し、この固定4 cell cutoffを置き換える。

## Rejected alternatives

- persistent material の内部 substep は gap / overlap をほとんど改善せず、実行時間と orphan を悪化させた。
- marker core の時間優先は churn spike を tick 47 から tick 57 へ先送りしただけだった。
- simulation `plate_id` の connected closure と spur erosion は、後続 tick の境界反応と shape を変えた。

実測と棄却理由は、関連する Rejected decision document に残す。

## Outcome

表示とsimulationで異なるplate partitionを持つ設計を撤回した。plate viewは正本の`plate_id`を直接表示する。
detached componentは表示時に隠さず、ownership changeをtopology-preserving transactionとしてcommitする
`260822-topology-preserving-material-front.md`で解消する。
