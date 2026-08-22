# Kinematic Euler boundary advection validation

## 対象

- 実装 decision: `docs/decisions/260822-kinematic-euler-boundary-advection.md`
- mesh: level 6
- 長期公開 pipeline: seed alpha、tick 0–1600
- 短期 seed 回帰: alpha、beta、gamma、delta、tick 0–160

## 旧実装の症状

alpha tick 0–160 の 34,671 transfer の 92.2% が次 tick に元の plate へ戻った。
tick 780–820 でも 8,582 transfer に対して区間差は 20 cell、即時反転率は 96.2% だった。
相対速度を現在の raster edge 法線へ射影して ownership 方向にしたため、cell transfer 後の法線変化が
同じ物理運動を逆向き候補へ変えていた。

## 比較した方式

- absolute Euler 速度、ordered plate pair、符号付き bucket accumulator を採用した。
- component の移動アンカー追跡は、広い対応半径で反転平均 0.115、最大 complexity 1.307、
  狭い対応半径で応答平均 0.128、最大 complexity 1.625 となり採用しなかった。
- accumulator の cap 1 は反転平均 0.0354、最大 0.1343、complexity 1.491、cap 4 は
  反転平均 0.0287、最大 0.1402、complexity 1.363 となり採用しなかった。
- tick 内 substep、front span cap の撤去、plate consistency projection の撤去は、反転、branch、
  component 分離のいずれかを悪化させたため採用しなかった。

## 160 tick seed 回帰

| seed  | 即時反転平均 | 即時反転最大 | 応答平均 | 最大 component | orphan | 最大 complexity |
| ----- | ------------ | ------------ | -------- | -------------- | ------ | --------------- |
| alpha | 0.0079       | 0.0463       | 0.1166   | 1              | 0      | 1.215           |
| beta  | 0.0090       | 0.0611       | 0.1445   | 1              | 0      | 1.293           |
| gamma | 0.0070       | 0.0608       | 0.1263   | 1              | 0      | 1.399           |
| delta | 0.0146       | 0.0737       | 0.1199   | 1              | 0      | 1.394           |

alpha tick 57 は全 9 plate が 1 component、orphan 0、最大 plate block 1、即時反転率 0.0187、
最大 complexity growth 1.043 だった。

## 時間刻み検証

Euler 角だけを時間比例させ、shape guard の cell/tick 上限を固定した候補では、Crust 末尾が
74.55 cell/tick、Environment 冒頭が 68.93 cell/tick だった。上限飽和が時間比例を打ち消していた。
front span と plate-level throughput 上限も 5 Myr 基準で縮尺した最終候補は次の結果になった。

| 区間      | 年代         | 実時間/tick | plate ID変更 cell/tick |
| --------- | ------------ | ----------- | ---------------------- |
| 760–800   | Crust        | 5 Myr       | 74.55                  |
| 800–840   | Environment  | 1 Myr       | 15.68                  |
| 1260–1300 | Environment  | 1 Myr       | 8.45                   |
| 1300–1340 | Life         | 1 kyr       | 0                      |
| 1395–1435 | Civilization | 100 yr      | 0                      |
| 1445–1485 | History      | 1 yr        | 0                      |

Crust から Environment の変更量比は 0.210 で、実時間幅の比 0.200 と整数 cell 量子化の範囲で一致する。
Life 以降の 40 tick で 0 cell なのは、level 6 の 1 cell 移動に必要な進行量へ達しないためであり、
残量は同じ時間刻みの間 accumulator に保持される。

## 1600 tick形状

| tick | plate数 | 最大component | orphan | 最小plate cell | 最大complexity growth |
| ---- | ------- | ------------- | ------ | -------------- | --------------------- |
| 0    | 9       | 1             | 0      | 625            | 1.000                 |
| 57   | 9       | 1             | 0      | 560            | 1.031                 |
| 160  | 9       | 1             | 0      | 160            | 1.267                 |
| 800  | 9       | 1             | 0      | 21             | 2.621                 |
| 840  | 9       | 1             | 0      | 21             | 2.613                 |
| 1300 | 9       | 1             | 0      | 24             | 2.567                 |
| 1395 | 9       | 1             | 0      | 24             | 2.567                 |
| 1445 | 9       | 1             | 0      | 24             | 2.567                 |
| 1600 | 9       | 1             | 0      | 24             | 2.567                 |

公開 pipeline の tick 840 transition guard は海陸連続性、水収支、sea level、continent continuity の
violation 0 を維持した。残った violation は既存の `land_freeboard_p90` 上限だけで、本変更とは分離する。
