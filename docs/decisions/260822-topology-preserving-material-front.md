# Topology-preserving shared plate front

## Status

Accepted

## Context

persistent material element は地殻物質の位置、面積、海洋性、年齢を保持できる。一方、plate ごとに
独立に剛体移流した element は全球の排他的 partition ではないため、収束で overlap、発散で gap が生じる。
marker を直接 rasterize して `plate_id` を作ると、この coverage ambiguity が離れた component と大きな
ownership change に変換される。alpha level 6 tick 57では、全球面積比で gap 15.8%、overlap 44.8%の
material projectionから1 tickに1,474 cellを再分類し、最大7 componentと7 orphan cellを作っていた。

GPlates は有限回転と時間依存の共有 plate boundary topology を分離する。front-tracking 法も、固定mesh上の
fieldとは別に共有interfaceを明示的に追跡する。digital topologyの連結性条件は、離散frontの局所変更が
sourceを分断せずtargetから孤立しないための判定に使える。

- Gurnis et al. (2012), _Plate tectonic reconstructions with continuously closing plates_,
  Computers & Geosciences 38, 35-42, doi:10.1016/j.cageo.2011.04.014.
- Unverdi and Tryggvason (1992), _A front-tracking method for viscous, incompressible,
  multi-fluid flows_, Journal of Computational Physics 100(1), 25-37,
  doi:10.1016/0021-9991(92)90307-K.
- Chen and Rong (2010), _Digital Topology on Adaptive Octree Grids_, IEEE Transactions on
  Visualization and Computer Graphics 16(2), 184-197, doi:10.1109/TVCG.2008.140.

## Decision

固定icosphereの排他的な `plate_id` partitionをruntime plate boundaryの正本にする。persistent materialは
地殻組成と年齢の輸送およびgap/overlap診断を担当し、marker rasterizationはownership authorityにしない。
表示も加工した別IDを作らず、simulationの `plate_id` をそのまま使う。

共有境界edgeの相対Euler速度をedge法線へ射影し、速度が向く側のcellをownership transfer候補にする。
候補はsource plate、target plate、球面上の安定bucketごとに連結componentへまとめる。sub-cell相当の移動量は
`BoundaryFrontAccumulatorState` に蓄積し、整数cellへ達した分だけcontiguous patchとして移す。

patchは次の条件をすべて満たす場合だけcommitする。

1. candidate cellがtarget plateへ2 edge以上で接している
2. donor plateがmesh規模に応じた最低cell数を維持する
3. patch除去後のsource plateが1 componentである
4. patch追加後のtarget plateが1 componentである
5. plate別のoutgoing、incoming、純面積変化がthroughput上限内である

throughput上限は `clamp(cell_count / 192, 8, 512)` cell/tick、純面積変化はその50%とする。
この上限はplate面積を固定するquotaではなく、未実装のsplit、merge、birth、lossをcell transferが暗黙に
引き起こさないための数値安全策である。transfer時のcellはtarget側の地殻種別を引き継ぐ。

## Alternatives

- material markerの最大支持をそのままownershipにする方式は、coverageの穴と重なりをtopology破壊へ変換した。
- cellごとのsimple-point逐次commitとboundary-length上限を組み合わせる方式は連結性を保ったが、frontが停止するか、
  alpha 120 tickのbranch、neck、multi-block gateを悪化させたため採用しない。
- detached fragmentだけ表示上で近傍plateへ塗り替える方式はsimulation stateを直さないため棄却した。
- persistent half-edge/DCELは連続境界をより忠実に表せるが、split/merge eventと球面交差解決まで含む実装コストが高い。
  現在の離散frontで満たせないsub-cell形状やplate lifecycleが必要になった時点で再検討する。

## Approximation

境界はbarycentric dual上のsub-cell polygonではなく、icosphere cell adjacency上の離散frontである。
front bucketは緯度経度を12分割した安定IDであり、連続曲線の物質点IDではない。速度積分の小数部はbucket単位で
保持するため、bucket境界をまたぐfrontでは残差の対応が近似になる。topology判定はcell graphの連結性を全球で
確認するが、連続球面の厳密なhomotopyやplate split/mergeは扱わない。

persistent materialのgap/overlapは、独立剛体移流と境界反応の未閉包量として残る。これは地殻物質モデルの
改善対象だが、排他的ownershipへ直接変換しないためplate shapeを破壊しない。
平均cell面積の0.01%未満のmaterialは既定どおり数値dustとして破棄する。ridge生成とsubduction切断でも
polygon再構成前に同じ閾値を適用し、次のprojectionで即時破棄する面積を退化polygonへ変換しない。

## Validation

12件のunit testで候補方向、fractional residual、component grouping、contiguous patch、source分断拒否、
target孤立拒否、donor floor、plate別throughput projectionを確認した。material reactionについても、
閾値未満のridge gapをpolygon化しないunit testを追加した。

alpha level 6では既存の120 tick shape gateを通過した。tick 57は9 plate、全plate 1 component、
orphan 0、detached fragment ratio 0、multi-block ratio 0で、1 tickのtransferは218 cellだった。
tick 160でも同じtopology条件を維持し、最大boundary complexity growthは1.023だった。

beta、gamma、delta level 6の160 tick seriesでも、全plate 1 component、orphan 0、detached fragment ratio 0、
multi-block ratio 0を維持した。最大boundary complexity growthはそれぞれ1.022、1.022、1.016だった。

alpha level 6の公開用precomputeをtick 1600まで完走し、1,600 deltaと26 keyframeを生成した。
候補storeと切替後のQuick Tunnelの両方でworldを作成し、tick 57へseekして40,962 cell、9 plateの
`plate_id` fieldを取得できることを確認した。

## Outcome

共有frontを唯一のownership authorityとして採用する。material raster由来の表示補正は削除する。
今後のmaterial gap/overlap改善は地殻物質収支の課題として扱い、`plate_id` の再構成とは分離する。
