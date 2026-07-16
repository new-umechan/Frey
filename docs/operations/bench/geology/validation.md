# Geology 検証運用

本書は `geology` の長期検証 bench をどう実行し、何を読んで、どこまでを pass/fail の判断材料にするかをまとめる運用文書である。
Geology の実装仕様は `docs/reference/`、設計変更の意図と検討状態は `docs/decisions/` を参照する。
Earth 実データ比較ベンチは `docs/operations/bench/geology/solo.md` を参照する。

## 目的

- 長期 Plate 挙動の破綻を早めに見つける
- Earth 類似の地殻構造が維持されているかを確認する
- 海面依存の見かけの変化と、地殻構造自体の破綻を切り分ける

## 使いどころ

- Geology の主要ロジックを変更したとき
- 主要パラメータを変更したとき
- `alpha_transition_guard` や hypsometry 系の異常を長期挙動まで遡って確認したいとき

## 前提

- 本 bench の主対象は海面そのものではなく、地殻の由来・年齢・境界過程である
- `land_ratio` や海岸線長は補助診断として読む
- `crust_type` が sea level 由来の派生量に戻っていないことを前提とする

## 主な評価軸

### 必須で見るもの

1. 大陸地殻と海洋地殻が共存していること
2. 大陸地殻と海洋地殻で高度分布が分離していること
3. 若い海洋地殻ほど浅く、古い海洋地殻ほど深いこと
4. 海嶺から離れるほど海洋地殻年齢が増加すること

### 補助で見るもの

- `land_ratio`
- 海岸線長
- 浅海面積
- hypsometry の崩れ方

## 違和感から見る指標

見た目や artifact に違和感があるときは、いきなり実装機構を疑わない。
まず現象を分類し、対応するモデル契約と診断指標を見る。
指標が契約を破っている場合だけ、右列の機構を調査する。

| 違和感                                 | まず見る契約                         | 主な指標                                                                                                                                  | 次に疑う機構                                                                 |
| -------------------------------------- | ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| プレートが斑点状に崩れる               | Shape stability                      | `component_count`, `detached_fragment_ratio`, `single_cell_plate_count`                                                                   | ownership transfer, donor guard, transfer reject condition                   |
| 同じプレートが複数の塊に割れる         | Shape stability                      | `plate_block_count`, `secondary_plate_block_ratio`, `multi_block_plate_ratio`                                                             | block budget, boundary crossing, plate compaction                            |
| 境界がギザギザに増える                 | Shape stability                      | `boundary_complexity`, `boundary_complexity_growth`, `persistent_boundary_complexity_growth`                                              | transfer scoring, boundary extraction, smoothing limiter                     |
| 細長い首や枝が残る                     | Shape stability                      | `corridor_neck_risk`, `appendage_isolation_risk`, `boundary_thin_cell_ratio`, `eroded_core_cell_ratio`                                    | shape guard, donor floor, split / merge candidate                            |
| プレートが止まって見える               | Temporal stability                   | `mean_plate_speed_km_per_myr`, `mean_cell_crossing_fraction_per_tick`, `boundary_crossing_substeps`                                       | force balance, basal floor, slab / ridge drive                               |
| 動きが往復や random walk に見える      | Temporal stability                   | `mean_direction_persistence`, `mutual_exchange_ratio`, `mean_centroid_path_straightness`                                                  | velocity update, ownership competition, drive memory                         |
| 境界移動が速度に追従しない             | Temporal stability                   | `boundary_motion_expected_cell_count`, `boundary_motion_actual_cell_count`, `boundary_motion_response_ratio`, `boundary_motion_runtime_*` | ownership front budget, CFL cap, component cap                               |
| 大陸と海洋の高さが混ざる               | Tectonic structure                   | hypsometry, `crust_type` distribution, age-depth trend                                                                                    | crust conversion, thermal subsidence, isostasy                               |
| 海嶺から離れても年齢や深さが変わらない | Tectonic structure                   | ridge age gradient, oceanic age-depth trend                                                                                               | ridge generation, boundary classification, oceanic crust age update          |
| 海陸がちらつく                         | Coastal and distribution diagnostics | `land_ratio`, shallow sea area, coastal response                                                                                          | sea level coupling, terrain projection, global height shift                  |
| 河川や湖が暴れる                       | Surface process stability            | sink / lake counts, storage, `river_next` changes, runoff response                                                                        | MFD rebuild, fill-spill, runoff spinup                                       |
| era 遷移で地形が跳ねる                 | Surface process stability            | transition tick deltas, erosion / deposition response, ice / water inventory                                                              | transition spinup, geology reflection order, glaciology / hydrology coupling |

## 実行タイミング

- 自動検証:
    - `cargo test`
    - `debug_assert!`
- 定量評価:
    - モデル・主要パラメータ変更時
- 定性評価:
    - 定量評価を通したあとに手動で読む

## Pre-plate emergence の確認

Damage-first 初期化を変更したときは、長期 bench の前に pre-plate emergence を先に確認する。
目的は、Crust runtime まで進める前に「弱い境界 network + 強い内部 block」が
初期化段階で成立しているかを切り分けることにある。

最低限、次を読む。

1. `plate_emergence_probe`
2. `crust_plate_count_series` の tick 0

`plate_emergence_probe` では `selected_valid_count` / `selected_regime` だけでなく、
`evolution_iterations[]` の checkpoint 推移を読む。
見るべき点は次である。

- base budget 前後で mobile-lid 候補がいつ立つか
- `selected_valid_count` が伸びたあと、追加 step で崩れていないか
- `settled_steps` が毎回 hard cap 側へ張り付き続けていないか
- `selected_multi_component_plate_count` が 0 を維持しているか
- `selected_mean_detached_fragment_ratio` が 0 近傍か
- `selected_final_plate_count` が target 帯から大きく外れていないか
- `selected_max_plate_area_ratio` が突出していないか
- `selected_effective_plate_count` が低すぎないか
- `selected_mean_plate_boundary_complexity` / `selected_max_plate_boundary_complexity` が
  seed 間で突出していないか

最後の 5 つは、plate 数が妥当でも shape や面積分布が現実離れしていないかを見るための補助指標である。
特に `multi_component_plate_count == 0` なのに complexity だけが高い場合は、
plate が分断されているのではなく、境界が細かく蛇行している問題と読む。
逆に `selected_valid_count` が十分でも `selected_max_plate_area_ratio` が高く
`selected_effective_plate_count` が低い場合は、
複数 block が見えていても最終 `plate_id` では少数の plate が支配している問題と読む。
この場合は score penalty を足す前に、
threshold candidate 群の中に area balance の良い候補が存在するかを先に確認する。

注意: `plate_emergence_probe` は `seed=earth` でも damage-first plate emergence を診断する。
一方で `build_geology_with_mesh("earth", ...)` は `earth_preset` へ早期 return するため、
`geology_validation_solo` default の `seed=earth` は probe の結果と一致しない。
Frey の通常生成 plate を Earth 実データと比較する場合は、`alpha` などの通常 seed を使う。

`crust_plate_count_series` は tick 0 だけでも、初期 plate field が runtime 前に
何 plate へ compact されたかを確認できる。
runtime まで見る場合は `plate_count` だけでなく、
`plate_id_churn_rate`、`orphan_cell_count`、`single_cell_plate_count` も合わせて読む。
plate 数が維持されていても `single_cell_plate_count` が増えるなら、
runtime ownership transfer が degenerate micro-plate を作っている可能性がある。
`component_count > 1` や `detached_fragment_ratio > 0` が runtime 後にだけ出る場合は、
boundary crossing の ownership transfer が plate を分断している可能性が高い。

2026-07-03 の `seed=eta` 観測では、修正前は tick 14 で plate 9 が
`component_count=2`, `detached_fragment_ratio=0.266355` になっていた。
原因は boundary crossing substep が stale な `plate_id_prev` で target support を判定し、
同じ substep 内で既に動いた隣接 cell をまだ support として数えていたことだった。
譲渡判定を live な `plate_id_next` に寄せた後、同じ 30 tick series では
multi-component event が 23 件から 0 件になった。

### Plate motion naturalness の確認

Crust runtime の plate が「形を保っているが、500万年/tick のスケールで動いていない」
ように見える場合は、`crust_plate_count_series` の motion 指標を読む。

例:

```bash
env CRUST_PLATE_SERIES_SEED=alpha \
CRUST_PLATE_SERIES_TICKS=120 \
CRUST_PLATE_SERIES_RECORD_EVERY=10 \
CRUST_PLATE_SERIES_BENCH_OUT=/tmp/frey_crust_plate_series.jsonl \
cargo run --manifest-path rust/Cargo.toml --bin crust_plate_count_series
```

主要列だけ読む:

```bash
jq -r '.samples[] | [
  .tick,
  .mean_plate_speed_km_per_myr,
  .max_plate_speed_km_per_myr,
  .mean_cell_crossing_fraction_per_tick,
  .boundary_crossing_substeps,
  .mean_direction_persistence,
  .reciprocal_churn_ratio,
  .mean_centroid_path_straightness,
  .mean_slab_pull_drive,
  .mean_ridge_push_drive,
  .mean_collision_drag,
  .mean_force_target_speed_km_per_myr,
  .mean_basal_target_speed_km_per_myr
] | @tsv' /tmp/frey_crust_plate_series.jsonl
```

plate ごとの駆動バランスを見る:

```bash
jq -r '.samples[-1].plates[] | [
  .plate_id,
  .cell_count,
  .area_ratio,
  .component_count,
  .detached_fragment_ratio,
  .boundary_complexity,
  .speed_km_per_myr,
  .cell_crossing_fraction_per_tick,
  .slab_pull_drive,
  .ridge_push_drive,
  .collision_drag,
  .force_target_speed_km_per_myr,
  .basal_target_speed_km_per_myr,
  .centroid_path_straightness
] | @tsv' /tmp/frey_crust_plate_series.jsonl
```

読み方:

- `mean_plate_speed_km_per_myr`
    - 現実の典型値は `20-100 km/Myr`、高速側で `100-150 km/Myr`
    - late Crust で `1 km/Myr` 未満へ落ちる場合は、5 Myr/tick に対して遅すぎる
- `mean_cell_crossing_fraction_per_tick`
    - 1 tick の移動量が平均セル間隔の何倍か
    - level 依存なので、同じ level の run 同士で比較する
- `boundary_crossing_substeps`
    - 1 tick 内で discrete ownership transfer を何分割したか
    - `mean_cell_crossing_fraction_per_tick > 1` で 1 のままなら、速度に対して境界移動が追従していない
- `mean_direction_persistence`
    - sample 間の plate velocity 方向 cosine
    - `0.7` 未満が続く場合は jitter / random walk を疑う
- `reciprocal_churn_ratio`
    - `from -> to` と `to -> from` の相互取り合いが少ないほど 1 に近い
    - 0 に近い場合は境界のせめぎ合いを疑う
- `mean_centroid_path_straightness`
    - `net displacement / cumulative path length`
    - 低いほど往復や蛇行が多い
- `mean_slab_pull_drive` / `mean_ridge_push_drive`
    - runtime kinematics が使った駆動力 proxy の平均
    - Frey では slab pull を主駆動、ridge push を副次駆動として読む
    - `mean_ridge_push_drive` が `mean_slab_pull_drive` より継続的に大きい場合は、
      plate speed が ridge activity に支配されすぎていないかを疑う
- `mean_force_target_speed_km_per_myr` / `mean_basal_target_speed_km_per_myr`
    - boundary force 由来 target と basal motion floor 由来 target の比較
    - basal 側だけで速度帯を維持している場合は、slab/ridge の分類や memory が弱すぎる可能性がある
- `plates[]`
    - plate ごとの speed / drive / target
    - 平均値が妥当でも、slab pull が強い plate が遅い、ridge-only plate が速すぎる、
      collision drag が高い plate が減速していない、といった外れ値を見る
    - `component_count > 1` や `detached_fragment_ratio` の上昇は、plate shape が
      時間経過で分断・崩壊している兆候として読む
    - `boundary_complexity` の上昇は、分断ではなく境界の蛇行・ギザギザ化を疑う

この指標は pass/fail gate ではなく、plate motion の自然さを読む診断 artifact とする。

2026-07-07 の block diagnostics 追加後、alpha/beta/gamma/delta level 6 の
160 tick run では、全 seed で `max_plate_block_count=1`、
`multi_block_plate_ratio=0`、`max_secondary_plate_block_ratio=0` だった。
同 run では全 seed が `boundary_motion_runtime_global_budget_cell_count=160` と
`boundary_motion_runtime_actual_transfer_cell_count=160` で一致し、
ownership transfer は global budget で律速していた。
これは現状の plate shape が robust multi-block には分かれておらず、
次の改善対象が block split ではなく budget 配分であることを示す baseline として扱う。

同日、`transferable_component_budget` 比例で global cap を広げる案を試した。
係数 `0.30` は response を約 `0.20` へ上げたが、beta で
`max_boundary_complexity_growth=1.269` になった。
係数 `0.20` でも beta/delta に 1 cell 級の detached component が出た。
このため cell-level 更新のまま adaptive global cap だけを広げる案は棄却した。

弱線ベース block 診断は、保存済みの `boundary_condition` と runtime stress を
weakness proxy とし、plate 内の上位 weakness 帯を cut として扱う。
同日の alpha/beta/gamma/delta 160 tick run では topology block は全 seed で 1 のまま、
weak-line block は beta/delta の一部 plate だけで非ゼロだった。
これは形状分裂前の内部弱線候補を拾う診断であり、まだ split/merge や budget 配分には使わない。

ownership 更新則は cell 単体の score 順 transfer ではなく、component 内で contiguous patch を作り、
source plate を分断せず、target plate から孤立しない場合だけ commit する。
これは transfer 数の調整ではなく、斑点状 fragment を作らないための更新単位の制約である。
同更新後は adaptive cap を残さず、mesh size 由来の hard cap に戻した。
alpha/beta/gamma/delta 160 tick run では topology block は全 seed で 1、
`boundary_motion_runtime_global_budget_cell_count=160` と
`boundary_motion_runtime_actual_transfer_cell_count=160` が一致した。
`boundary_motion_runtime_patch_rejected_budget_cell_count` は beta の 1 cell を除き 0 で、
その beta も `target_disconnected` による 1 cell 拒否だった。
response は約 `0.11-0.13` に残り、shape guard ではなく global hard cap が
motion response の主な律速である。
次に budget を広げる場合は、patch-level guard の下で再検証し、
topology block、weak-line block、boundary complexity が悪化しないことを先に確認する。

同条件で hard cap の mesh divisor を比較した。
`cell_count / 192` は response を約 `0.14-0.18` へ上げたが、
beta の `max_boundary_complexity_growth=1.285` が gate の `1.25` を超えたため棄却した。
`cell_count / 224` は complexity gate を通したが、delta の
`max_weak_line_plate_block_count=8` が baseline より強く悪化したため採用しない。
`cell_count / 240` は level 6 で global budget 170 cell/tick となり、
alpha/beta/gamma/delta 160 tick run で topology block は全 seed で 1、
`persistent_boundary_complexity_growth_plate_ratio=0`、最大 complexity は beta の `1.153` に収まった。
response 改善は約 `0.11-0.14` と小さいが、shape stability を優先する保守的な上限として採用する。

その後、global cap を削除し、boundary component の local flux proposal を
plate-level consistency projection で縮小する方式へ切り替えた。
projection は plate ごとの incoming/outgoing throughput cap と donor floor を使い、
world 全体の transfer quota は持たない。
throughput divisor `128` は response を約 `0.18-0.22` へ上げたが、
delta の `max_weak_line_plate_block_count=9` が強かった。
divisor `160` は response 約 `0.15-0.18`、delta weak-line block 7。
divisor `192` は response 約 `0.12-0.17`、全 seed で topology block 1、
`persistent_boundary_complexity_growth_plate_ratio=0`、最大 complexity は alpha の `1.049`、
delta weak-line block 6 だった。
local projection の初期値としては divisor `192` を採用し、次の改善は
単純な cap 調整ではなく component / plate ごとの rejected flux の内訳を見る。

projection diagnostics と net area delta cap を追加した後の同条件 run では、
response は約 `0.13-0.16`、全 seed で topology block 1、
`persistent_boundary_complexity_growth_plate_ratio=0`、最大 complexity は gamma の `1.026` だった。
weak-line block は beta/delta が 4 で、net cap 前の delta 6 より下がった。
tick 160 時点の projection deferred は主に outgoing throughput と incoming throughput によるもので、
`net_area_limited` は 0 だった。
このため現時点では net area cap は safety guard として残し、次の調整対象は
plate-level throughput の配分と component priority とする。

境界が全体的に行き戻りして見える問題に対して、candidate 生成を cell-local から
undirected edge-local に変更した。
同じ edge では signed normal flux を一度だけ評価し、両側の takeover proposal を
同時に出さない。
alpha/beta/gamma/delta 160 tick run では topology block と complexity は安全側だったが、
`reciprocal_churn_ratio` はまだ `0.05-0.10` 程度で、plate pair 間の相互取り合いは残った。
同じ local pair/bucket 内の reverse flux を netting する案も試したが、
response が `0.01-0.02` まで落ち、境界移動がほぼ止まったため棄却した。
次にこの問題を見る場合は、単純な reverse flux cancel ではなく、component identity の時間継続性か
material/tracer 的な移流へ寄せる必要がある。

plate material advection の初期実装では、cell ごとに primary/secondary material を持ち、
Euler velocity で近傍 material を半 Lagrangian 的に混ぜてから dominant material で
`plate_id` を復元する。
material-only reconstruction は reciprocal churn と straightness を改善したが、
delta で `max_plate_block_count=29`、`max_boundary_complexity_growth=2.63` まで悪化したため
棄却した。
material mixing を弱め、既存の topology-safe boundary cleanup の前段として使う方式では、
mixing cap `0.16` の alpha/beta/gamma/delta 160 tick run で topology block は全 seed で 1、
`persistent_boundary_complexity_growth_plate_ratio=0`、最大 complexity は delta の `1.182` に収まった。
`reciprocal_churn_ratio` は約 `0.18-0.29`、`mean_centroid_path_straightness` は約 `0.61-0.91` まで改善した。
このため material advection v1 は topology-free replacement ではなく、
cleanup 前の rigid-like motion preconditioner として採用する。

semi-Lagrangian backtrace sampling も試した。
2-hop source sampling は response を約 `0.50-0.60` へ上げたが、
alpha/gamma/delta で topology block が 20 以上に増え、boundary complexity も大きく悪化した。
1-hop に制限し backtrace step を短くしても、beta/delta の area delta と complexity が悪化した。
このため backtrace sampling は現時点では棄却し、local material mixing `0.16` を維持する。
backtrace に再挑戦する場合は、dominant reconstruction 前に conservative remap か topology-aware
material clipping が必要である。

material mixing を current `plate_id` の境界帯だけに限定し、上流 neighbor material だけを混ぜる案も試した。
alpha/beta/gamma は topology block 1 に収まったが、`reciprocal_churn_ratio` は
約 `0.09-0.15` まで落ち、local material mixing `0.16` より悪化した。
delta では `max_plate_block_count=3` になったため棄却した。
境界帯だけを使う場合も、単純な上流混合ではなく、mass-conservative な interface remap と
topology-aware clipping を組み合わせて再設計する必要がある。

`reciprocal_churn_ratio` は pair 間の net one-way dominance を見る指標であり、
値が低いほど相互交換が多いことを意味する。
このため直接的な往復診断として `mutual_exchange_ratio` を追加した。
artifact では同じ値を意味が明確な `net_exchange_directionality_ratio` としても出力する。
`temporal_reversal_ratio` は、前 sample で所属変更した同一 cell が次 sample で元の plate に
戻った割合を測り、別々の境界区間で同時に起こる正当な双方向面積交換と区別する。
`nearest_centroid_voronoi_agreement_ratio` は各cell所属が現在のplate重心によるVoronoi分類と
一致する割合、`centroid_voronoi_energy_ratio_from_initial` は重心までの二乗球面距離総和を
tick 0で正規化した値である。これは単独で地学的な正誤を決める指標ではないが、物理過程を
持たないcentroid relaxationへの長期収束を検出する。400 tick以上ではenergy ratioが`0.75`を
下回ったrunをwarningにする。閾値はEarth類似度ではなく、初期形状からの過度な幾何学化を
拾う保守的なregression gateである。
過去のinfluence modelのalpha level 6、400 tick、10 tick間隔runでは、plate数9、最大block 1、
孤立cell 0を維持した一方、energy ratioはtick 50で`0.592`、tick 400で`0.607`、
nearest-centroid一致率はtick 0の`0.648`からtick 400の`0.804`へ上がった。
したがって長期の幾何学化は分裂・枝とは独立したregressionとして検出できる。
過去の local material mixing `0.16` + topology cleanup の alpha/beta/gamma/delta
160 tick run では、`mutual_exchange_ratio` は約 `0.89-0.90` だった。
同 run では response は約 `0.16-0.18`、topology block は全 seed で 1、
最大 complexity は delta の `1.185` に収まった。
既存 accumulator に full-cell commit threshold と reverse residual debt を足す
directional hysteresis も試したが、alpha/beta の途中結果で response は
約 `0.09-0.12` まで落ち、straightness は約 `0.19-0.29` に悪化した。
alpha は `max_boundary_complexity_growth=1.29` で gate を超えたため棄却した。
次に hysteresis を試す場合は fractional commit を hard gate 化せず、
soft な reverse-debt state として設計し直す。

過去の既定 ownership は persistent influence generator 方式だった。generator を Euler 回転で進め、
現在領域重心へ `0.2` 緩和し、local candidate と初期面積補正 `0.18` で所属を再分類していた。
alpha/beta/gamma/delta level 6、160 tick、毎 tick 記録では、最終 plate 数は `9/9/7/7`、
速度は `77-84 km/Myr`、方向持続性は全 seed `0.999` 以上、response は `0.87-1.54` だった。
最終 block は全 seed で 1、complexity growth は `1.02-1.06`、面積成長は `1.46-1.89` で
全 warning gate を通過した。`mutual_exchange_ratio` は `0.064-0.239`、straightness は
`0.624-0.815`、Euler residual ratio は `0.47-0.71` であり、厳密な剛体 polygon 移流ではない。
alpha/beta/gamma には一時的な微小第2成分があり、alpha の履歴最大面積成長は `3.81` だった。
この方式は比較検証の結果、現在の実行経路から削除し、persistent material element 方式へ固定した。

surface material parcel prototype の1 tick dry runは次で実行する。

```bash
pnpm bench:probe:surface-material
```

2026-07-10の`seed=alpha`, level 6では、nearest-cell remap直後に全40,962 cell中
4,855 cellが空、4,528 cellが重複した。previous boundary端点外だけでも空cellは4,030、
重複cellは3,761あり、endpoint reaction後もこの内部値は変わらなかった。
nearest-cell remapは棄却する。

球面三角形への保存的projectionとcenter+neighbor quadrature parcelを使うと、alphaの未被覆は
2,312 cellまで減った。全表面を単一plateとして同じEuler回転を与えたlevel 6 controlでは、
未被覆とtriangle fallbackはいずれも0だった。したがってprojectionはrigid plate内部を覆えており、
alphaに残る未被覆は独立回転したplate partition間のgapである。
previous boundary端点だけを広げるのではなく、previous positionからadvected positionまでの
swept boundary areaをrasterizeして発散・沈み込みreactionへ渡す。

同日の再検証で、relative normal velocityの符号が逆に解釈されていたことが判明した。
隣接cell間距離の時間微分は正が発散、負が収束である。解析テストで規約を固定して分類を
修正すると、表示cellと同じbarycentric dual-edgeのswept stripによる発散reactionは
未被覆を2,312から76へ減らした。残差はquadratureの限界ではなく、3 plate junctionで
1つの旧dual vertexが3点へ移動して生じる三角形の開口だった。junction polygonを追加すると
quadratureは2,312から0、dual-cell overlap remapは2,686から0になった。overlap remapは
全40,962 source cellを未割当0、invalid 0でdepositした。次の未解決処理はsubduction overlapである。
subduction reactionは1,274 cellから海洋material 1,149.23 massを除去し、mixed cellを
4,278から3,121へ減らした。当初の2件の拒否は、海洋性endpointと同じplateでもswept cellの
local materialが大陸性だったためで、proposalをlocal oceanic mass必須にして解消した。
collision/transform由来の残存mixを分けてからruntimeへ接続する。
subduction後の3,121 mixed cellをexclusiveに分類すると、collision 3,064、subduction 8、
divergent 34、sweep外15だった。collision 3,064の組成は大陸-大陸839、大陸-海洋1,263、
海洋-海洋962である。したがってcollision全体を地殻厚化してはならない。現行classifierでは
age/density gateを通らない海洋収束もcollisionへ入るため、遅延subduction・obduction・
大陸衝突を分離してから処理する。
edge単位の収束regimeを分離すると、legacy collision 945本は大陸衝突274本と
開始前subduction 671本に分かれ、既存subduction 232本はactive subductionと一致した。
開始前subductionは候補海洋plateを保持し、収束memoryと負浮力条件による遷移を検証する。

収束regimeの時系列は次で実行する。

```bash
CONVERGENT_REGIME_SERIES_SEED=alpha \
CONVERGENT_REGIME_SERIES_TICKS=160 \
pnpm bench:series:convergent-regime
```

alpha level 6の160 tickではreclassify間隔4 tickに合わせて遷移が集中した。tick 5では
開始前からactiveへの遷移328本、逆遷移15本が出たが、tick 21以降は両方向が近い小規模遷移に
なった。開始前subduction edgeはtick 21の41本からtick 160には95本へ増えた。従って現行の
開始前labelは持続的な開始状態ではなく、再分類時の瞬間出力として読む。開始過程を採用するには
ownership transferとは別のedge-local progress stateを設計する。
progress state導入後のalpha level 6では、開始前subduction edgeはtick 1の671本からtick 160の
21本へ減り、active subductionは232本から818本へ増えた。tick 5以降の逆遷移はほぼ0である。
この結果はcommit状態の持続を示すが、active subduction量が大きく変わるため、surface materialと
plate shapeの長期seriesをruntime採用前に必ず確認する。

progress state導入後のalpha 160 tick shape seriesでは、最大plate block数1、multi-block比0、
single-cell plate 0、最大boundary complexity growth 1.038、最大appendage risk 0.002未満だった。
同じtick 160 stateのsurface-material dry runでは、発散が2,525 cellへ1,739.52 massを生成し、
subductionが1,875 cellから1,273.79 massを除去した。未被覆とsubduction rejectはいずれも0、
net surface massは+465.73である。collision excessとの対応を確認するまでruntime採用しない。

任意tick後のsurface-material probeは次で保存できる。

```bash
SURFACE_MATERIAL_PROBE_TICKS=160 \
SURFACE_MATERIAL_PROBE_OUTPUT=/tmp/frey_surface_tick160.json \
pnpm bench:probe:surface-material
```

2026-06-23 の basal motion floor 導入後、alpha level 6 の 80 tick run では
`mean_plate_speed_km_per_myr` が tick 20 以降も `42-63 km/Myr` に残った。
導入前は tick 40 以降で `0.1 km/Myr` 前後まで落ちていたため、
late Crust の速度崩れは解消したと読む。
同じ run で `mean_cell_crossing_fraction_per_tick` は `1.7-2.6` 程度なので、
次に見るべき点は複数セル相当の displacement と ownership transfer の整合である。

同日の drive 正規化と boundary crossing substep 導入後、bench 側の速度計算を
runtime velocity と同じく `activity` 非依存に揃えた alpha level 6 の 80 tick run では、
tick 80 で `mean_plate_speed_km_per_myr=70.0`、
`mean_cell_crossing_fraction_per_tick=2.91`、`boundary_crossing_substeps=4` だった。
同 tick の drive は `mean_slab_pull_drive=0.213`、
`mean_ridge_push_drive=0.093` で、ridge push は無視できるほど小さくはないが
slab pull より小さい副次駆動として読める。
`mean_force_target_speed_km_per_myr=57.4` と
`mean_basal_target_speed_km_per_myr=56.6` が近く、速度維持が basal floor だけに
依存していないことも確認できる。
per-plate では slab が強い plate 2/3/4 が `75-90 km/Myr` と速く、
force target も高い。一方で plate 7 は `mean_slab_pull_drive=0` でも
basal target により `57 km/Myr` 程度で動くため、basal proxy が強すぎる
外れ値を今後の確認対象とする。

同日の boundary crossing shape guard 導入後、alpha level 6 の 120 tick run では
全 plate が `component_count=1`、`detached_fragment_ratio=0` を維持した。
導入前は tick 80 で `max_components=4`、`max_detached=0.333` まで悪化していた。
guard 後も `boundary_complexity` は最大 `14.8` まで上がるため、
残る違和感は分断よりも境界の蛇行・細かさとして確認する。

自動化可能な配列整合性、値域、決定性、snapshot 整合はコード側で担保する。
本書では手動サニティチェック手順を保持しない。

## 結果の読み方

- 大陸地殻と海洋地殻の共存が崩れた場合:
    - 地殻生成や変換ロジックの破綻を先に疑う
- 高度分布の分離だけ崩れた場合:
    - hypsometry か isostatic / smoothing 系の悪化を疑う
- age-depth 傾向だけ崩れた場合:
    - 海洋地殻の冷却・沈降や年齢更新の破綻を疑う
- ridge age gradient だけ崩れた場合:
    - 海嶺近傍の生成・更新順序や境界分類を疑う
- 補助診断だけ悪化した場合:
    - 海面依存の変化か、主評価軸の悪化に引きずられた副作用かを切り分ける

## 関連文書

- `docs/operations/bench/geology/solo.md`
- `docs/operations/bench/geology/validation_solo.md`
- `docs/operations/bench/geology/legacy_hypsometry_handover.md`
- `docs/operations/bench/glaciology/alpha_transition_guard.md`

## 学術アンカー

- USGS, water coverage
- NOAA NCEI, hypsographic curve / ETOPO
- Stein & Stein (1992), oceanic age-depth
- NOAA NCEI, ocean crust age dataset

閾値調整や指標見直しが必要になった場合は、まず上記アンカーと現在の artifact を照合して判断する。
