# Terrain naturalness understanding loop

## Status

Draft

## Context

Frey が扱う範囲は、地質、地形、水文、気候、生態、人類活動まで広い。
そのため、すべての理論とすべての実装詳細を同時に理解する前提では、
地形の違和感を判断できなくなる。

現在の主な問題は次である。

- 時間経過で小さな近似誤差や proxy の偏りが蓄積する
- どこまでが正しい挙動で、どこからが破綻なのかを定義しきれていない
- 地形が不自然に見えても、理論不足、モデル契約不足、実装不具合のどれかを切り分けにくい
- 低レイヤー実装の複雑さに、人間の判断が引きずられやすい

Frey では、低レイヤーをすべて理解するのではなく、
高いレイヤーから理解し、低いレイヤーは契約と診断で封じ込める。

## Proposed Decision

地形改善の判断を、次の 3 層で扱う。

```text
L1: Phenomenon
    人間が見たい現象、違和感、世界観。

L2: Model Contract
    Frey が守る近似、保証、非保証、診断指標。

L3: Mechanism
    実装アルゴリズム、更新順序、最適化、内部状態。
```

改善判断の正本は L1 と L2 に置く。
L3 は、L2 の契約を満たしているかを検証するための実装詳細として扱う。

地形の改善は、単に見た目が自然になったかではなく、
次の問いに答えられる状態を目標にする。

1. L1 のどの違和感を改善対象にしているか
2. その違和感は L2 のどの契約で評価できるか
3. 既存の L2 契約で評価できない場合、どの契約を追加するか
4. L2 契約が壊れている場合、L3 のどの機構を疑うか
5. L2 契約が通っているのに L1 が不自然な場合、どの現象理解が不足しているか

## L1 terrain phenomena

地形について、人間がまず判断する対象は次とする。

- 大陸、海盆、山脈、海溝、海嶺、平原、河川、湖、デルタが読める
- 山脈や海溝が plate boundary と関係して見える
- 大陸と海洋の高さ分布が混ざりすぎない
- 海岸近傍が平坦すぎたり急すぎたりしない
- 河川が高地から低地へ向かい、流域として読める
- 時間経過で plate が止まったり、地形がノイズ化したりしない
- era 遷移で hydrology や glaciology が地形へ shock を与えない

この層では、厳密な物理式ではなく「何が不自然に見えるか」を保持する。

## L2 model contracts

L1 を直接 gate しない。
Frey では、L1 の違和感を次の L2 契約へ落とす。

### Tectonic structure

- 大陸地殻と海洋地殻が共存する
- 大陸地殻と海洋地殻の hypsometry が分離する
- 若い海洋地殻ほど浅く、古い海洋地殻ほど深い
- ridge から離れるほど海洋地殻年齢または深度が増える
- convergence / subduction / ridge / transform の boundary response が relief と対応する

### Shape stability

- plate は runtime 中に多成分化しない
- degenerate micro-plate が増え続けない
- boundary complexity が極端に増え続けない
- donor floor や shape guard が plate の視覚的崩壊を抑える

### Temporal stability

- plate speed は Crust 後半でも現実的な桁から外れすぎない
- direction persistence が低すぎる random walk にならない
- reciprocal churn が境界の取り合いを示し続けない
- tick 途中の ownership transfer と displacement が整合する

### Surface process stability

- Hydrology は地形、海面、runoff の変化に対して sink / lake / spill を再現性ある形で更新する
- fluvial deposition は erosion と mobile sediment budget を超えて増えない
- Environment 期以降は、海陸比を合わせるための全球 terrain shift を行わない
- era 遷移では runoff と erosion / deposition response を spinup で遷移させる

### Coastal and distribution diagnostics

- coastal inundation response が reference terrain と大きく乖離しすぎない
- land ratio、浅海面積、hypsometry、relief distribution を補助診断として読む
- 全球 height RMSE を主評価にしない

## L3 mechanisms

L3 は常時理解する対象ではなく、L2 の異常から逆引きする対象とする。

例:

- age-depth が壊れた場合:
    - oceanic crust age update
    - thermal subsidence proxy
    - ridge generation / classification

- plate が小片化した場合:
    - boundary crossing substep
    - donor floor
    - transfer reject condition
    - plate ownership compaction

- 河川や湖が暴れる場合:
    - MFD rebuild mode
    - sink full / incremental rebuild
    - sanitize primary next
    - fill-spill storage

- era 遷移で地形が跳ねる場合:
    - runoff spinup
    - erosion / deposition response spinup
    - sea level capacity closure
    - Geology 反映順序

## Concrete Tasks

### Task 1: L1 違和感カタログを作る

`docs/reference/modules/geology.md` または新規 reference 文書に、
地形の違和感を分類する一覧を作る。

最低限、次の列を持つ。

- symptom
- example
- suspected L2 contract
- primary diagnostic
- owner module
- out of scope

完了条件:

- 地形の違和感を見たとき、まずどの分類へ入れるかを選べる
- 「なんとなく不自然」で止まらない

### Task 2: L2 契約表を geology / hydrology に分ける

既存の geology validation と hydrology reference を整理し、
各契約を次の形で明示する。

- contract name
- phenomenon covered
- diagnostic artifact
- acceptable band or reading rule
- known limitation
- related L3 mechanism

完了条件:

- L1 の主要違和感が、少なくとも 1 つの L2 契約へ対応する
- L2 契約で扱えない違和感は `uncovered` として明示される

### Task 3: terrain naturalness triage を運用文書化する

`docs/operations/bench/geology/validation.md` に、
地形違和感を見つけたときの triage 手順を追加する。

手順は次の順序にする。

1. L1 symptom を選ぶ
2. 対応する L2 contract を確認する
3. contract が fail しているか、contract 自体が不足しているかを分ける
4. fail している場合は L3 mechanism を調べる
5. contract が不足している場合は Draft decision を作る

完了条件:

- 地形が変だと感じたとき、次に見る artifact が決まる
- 実装へ潜る前に、契約不足か実装不具合かを分けられる

### Task 4: 長期誤差蓄積の sentinel を決める

長期 run で必ず記録する sentinel を固定する。

候補:

- `mean_plate_speed_km_per_myr`
- `mean_cell_crossing_fraction_per_tick`
- `boundary_crossing_substeps`
- `mean_direction_persistence`
- `reciprocal_churn_ratio`
- `mean_centroid_path_straightness`
- `multi_component_plate_count`
- `detached_fragment_ratio`
- `boundary_complexity`
- `land_ratio`
- `shallow_sea_ratio`
- `hypsometry_distance`
- `sediment_budget_ratio`
- `coastal_deposition_share`
- `low_slope_deposition_share`

完了条件:

- 最終 tick だけでなく、途中 tick で破綻の始まりを読める
- どの sentinel がどの L2 契約に対応するかが文書化されている

### Task 5: visual inspection を契約へ接続する

地形画像や UI 上の違和感を、L1 symptom と L2 contract に紐づける。

完了条件:

- スクリーンショットや観察メモが、診断不能な主観で終わらない
- 見た目の違和感から、読むべき JSONL / score / diagnostic へ戻れる

### Task 6: uncovered symptom を decision 化する

L1 で違和感があるのに L2 契約がない場合、
個別実装修正に入らず Draft decision を作る。

完了条件:

- L2 契約なしに L3 実装をいじらない
- モデル改善と実装不具合修正が混ざらない

### Task 7: 改善終了条件を PR / 作業単位に入れる

地形改善作業には、次の終了条件を持たせる。

- 対象 L1 symptom が明示されている
- 対応する L2 contract が明示されている
- 改善前後の diagnostic が残っている
- L2 contract が改善したか、または不足していることが分かった
- L3 の変更が L2 のどの契約へ効いたかを説明できる

完了条件:

- 「見た目が少し良くなった気がする」だけで完了しない
- 逆に、低レイヤーを完全理解しなくても改善完了を判断できる

## First Milestone

最初の milestone は、実装改善ではなく判断可能性の確立とする。

1. L1 違和感カタログを追加する
2. geology / hydrology の L2 契約表を作る
3. terrain naturalness triage を validation 運用に追加する
4. 長期 sentinel と L2 contract の対応表を作る

この milestone が終わるまでは、地形の見た目改善を大きく進めない。
先に、人間が「どこまで改善すればよいか」を判断できる状態を作る。

## Consequences

利点:

- 理論をすべて理解しなくても、判断に必要な契約を読める
- 実装詳細をすべて追わなくても、L2 の fail から L3 を逆引きできる
- 地形の違和感を、主観ではなく改善タスクへ落とせる
- L2 契約が不足している場合に、実装ではなく設計へ戻れる

欠点:

- 初期段階では文書化コストが増える
- L1 の違和感を L2 契約へ落とせないケースが残る
- 数値指標を増やしすぎると、逆に人間が読めなくなる

## Out of Scope

- 完全な地球物理 solver
- すべての地形美の自動評価
- 全球 height RMSE を主目的にした最適化
- UI 上の見た目だけを基準にした tuning
- L3 実装詳細をすべて人間が常時理解すること

## Close when

- L1 symptom、L2 contract、triage 手順を採用する場合は `Accepted` にし、現在仕様を `docs/reference/modules/geology.md` と `docs/reference/modules/hydrology.md`、運用手順を `docs/operations/bench/geology/validation.md` へ反映する。
- 地形判断の整理を別の用語体系へ寄せる場合は `Superseded` にし、置換先を明示する。
- L1/L2/L3 の分類を採用しない場合は `Rejected` にし、理由を残す。
