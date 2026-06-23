# Convergence-monitored pre-plate damage evolution

## Status

Accepted

## Context

Damage-first plate emergence は `pre_plate_steps` 固定の base pass のあと、
一部 seed にだけ tail pass を足していた。

この方式には次の問題があった。

- 追加 pass の発火条件が `valid_count` と `largest_ratio` の閾値に依存していた
- `gamma` のように base budget の手前で一度よい mobile-lid 候補が出ても、
  base pass 完了時点の snapshot が悪いと捨てやすかった
- 「いつ止めるか」の根拠が plate field 自体の収束ではなく、個別 seed の症状寄りだった

## Decision

Pre-plate damage evolution は fixed tail pass をやめ、checkpoint 付きの収束監視へ切り替える。

- `pre_plate_steps` は base budget として使う
- evolution は 8 step ごとに checkpoint を取り、`boundary_potential` から
  `BoundaryExtraction` を再評価する
- 最良の extraction は全 checkpoint を通して保持する
- base budget 到達後は、best extraction が 2 checkpoint 連続で更新されなければ停止する
- 上限は `pre_plate_steps * 2`、ただし hard cap は 160 step とする

best extraction の比較は plate count target ではなく、構造指標で行う。

- regime が異なる場合は `regime_score` が小さい方を優先
- `mobile_lid` 同士では `valid_count` が多い方を優先
- 同数なら `largest_ratio` が小さい方を優先
- それでも同等なら `regime_score` が小さい方を優先

## Consequences

利点:

- 追加進化の発火条件が個別 seed 向け閾値から、plate field の改善停滞へ移る
- base budget 前に得た良い mobile-lid 候補を保持できる
- `plate_emergence_probe` で step ごとの収束履歴を定量確認できる

欠点:

- 停止条件はなお近似であり、厳密な物理収束判定ではない
- checkpoint 評価と snapshot 保持の分だけ初期化コストは少し増える
- best extraction 比較順序は現段階では経験的で、graph/skeleton 導入後に見直し余地がある
