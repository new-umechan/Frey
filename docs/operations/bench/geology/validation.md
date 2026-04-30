# Geology 検証設計

`benchmark.md` の Geology 節を補完する、長期 Plate 専用の検証ドキュメント。
Earth 実データ比較ベンチ（`geology_solo`）は `docs/operations/bench/geology/solo.md` を参照する。
本書は、重い実データ一致ベンチではなく、Earth 類似の物理整合性をみる軽量な定量評価と、ウィルソンサイクル（WC）の定性評価を組み合わせて運用する。

---

## 位置づけ

Geology は Climate/Hydrology/Ecology のような実測値との直接比較ベンチマークの対象外である。
代わりに、以下で妥当性を担保する。

| 種別                    | 目的                                         | 実行タイミング                 |
| ----------------------- | -------------------------------------------- | ------------------------------ |
| **自動検証（コード）**  | 実装破綻を早期に検知する                     | `cargo test` / `debug_assert!` |
| **定量評価（軽量）**    | 海面変動に依らない地質構造の整合性を確認する | モデル・主要パラメータ変更時   |
| **WC 定性評価（手動）** | 長期挙動が「それらしいか」を判断する         | 定量評価通過後                 |

---

## 自動検証（コードへ移管済み）

従来のサニティチェックのうち、自動化可能な項目はドキュメント運用から外し、コード側へ移した。

- 配列整合性: geology/hydrology/runtime geology の配列長整合を `debug_assert!` で検証
- 値域・有限性: `height`、`mantle_heat`、各地殻/境界状態の値域・NaN/Inf を `debug_assert!` で検証
- `river_next` ループ: `debug_assert!` でサイクルを検出
- 決定性: 同一 seed + params + 更新スケジュールでの一致を `cargo test` で検証
- シリアライズ: serialize/deserialize の snapshot 一致と復元後 step 一致を `cargo test` で検証

このため、本書では手動サニティチェック手順を保持しない。

---

## 定量評価（Earth 類似の物理整合性）

### 基本方針

Geology の定量評価は、海面変動そのものではなく、**地殻の由来・年齢・境界過程** を主対象にする。

- 主評価は **地殻 provenance 基準** とする
- 大陸/海洋の判定は `height > sea_level` ではなく、`crust_type` の**生成履歴上の意味**に基づいて行う
- `land_ratio`、海岸線長、浅海面積のような海面依存量は補助診断に下げる
- ただし、`crust_type` が `height` から再導出される実装のままでは、主評価として使ってはならない

理由:

- この時間スケールでは海面上昇・下降により見かけの海陸比が大きく変動しうる
- tectonics の主評価対象は、海岸線そのものではなく、大陸地殻と海洋地殻の生成・維持・更新である
- 一方で、`crust_type` が `height` 起源の派生量にすぎない場合、評価は sea level 非依存ではなくなる
- したがって、Geology の pass/fail は「海面から独立した地殻属性が保持されている」ことを前提条件にしなければならない

### 調べる

Earth 類似の plate tectonics を定量評価するうえで、最低限みるべき一次構造は次の 4 つである。

1. 大陸地殻と海洋地殻の共存
2. 大陸地殻と海洋地殻で高度分布が分離していること
3. 若い海洋地殻ほど浅く、古い海洋地殻ほど深いこと
4. 海嶺から離れるほど海洋地殻年齢が増加すること

これらは、海面が動いても tectonics の健全性を比較的安定して表す。

参考:

- 現在の大陸地殻面積は地球表面の約 40% 付近で長期平衡にあるというレビューがある
- Earth の表面高度分布は、大陸高地と海盆低地に対応する分離構造を持つ
- 海洋地殻は年齢とともに冷却・高密度化し、海洋底は若いほど浅く古いほど深くなる一次傾向を示す
- 最も若い海洋地殻は海嶺に沿って分布し、古い海洋地殻は海溝・受動縁辺近傍へ向かって増加する

### 案を出す

候補を次の 4 案で整理する。

| 案  | 内容                                                                         | 採否         |
| --- | ---------------------------------------------------------------------------- | ------------ |
| A   | `land_ratio` や海面上セル比で評価する                                        | 不採用       |
| B   | `crust_type` を基準に地殻構造を評価し、海面依存量は別管理する                | 条件付き採用 |
| C   | 全球相対高度分布のみで hypsometry をみる                                     | 不採用       |
| D   | `crust_type` 条件付き高度分布、海洋 age-depth、ridge age gradient を評価する | 採用         |

不採用理由:

- 案 A は海面変動に弱く、地質の破綻と海水面変動を区別しにくい
- 案 C は「地球らしい二峰性」の物理的意味を捨ててしまい、大陸地殻と海洋地殻の分離を直接検証できない

採用理由:

- 案 B は、`crust_type` が sea level から独立した状態量として実装されている場合に限り有効である
- 案 D により、海面が動いても「大陸地殻は相対的に高く、海洋地殻は相対的に低い」「若い海洋地殻は海嶺にあり浅い」という tectonics 本体の性質を確認できる

### 検証する

定量評価の最小セットとして、次の 4 指標を採用する。

#### 前提ゲート: `crust_type` 独立性

定量評価に入る前に、次を満たすことを確認する。

- `crust_type` が `height` や `sea_level` から再導出されていない
- 海洋地殻化は「浸水したから」ではなく、発散境界での新生 oceanic crust 生成として定義されている
- 大陸地殻化は「露出したから」ではなく、衝突・付加・肥厚などの地殻過程として定義されている

満たさない場合:

- 本節の Q-1〜Q-4 は **参考値** 扱いに格下げする
- pass/fail 判定を出してはならない

#### Q-1: 大陸・海洋地殻面積比

指標:

- `continental_crust_fraction`
- `oceanic_crust_fraction`

定義:

- `continental_crust_fraction`: `crust_type == Continental` のセル面積比
- `oceanic_crust_fraction`: `crust_type == Oceanic` のセル面積比
- 両者は地殻種別の面積収支をみるための対になる量として扱う
- `land_ratio` は使わない
- ただし `crust_type` が provenance を表す内部状態であることを前提とする

判定:

- 100 tick 観測窓で、`continental_crust_fraction` と `oceanic_crust_fraction` の変動を記録する
- Earth preset では、どちらか一方が極端に支配的になり続けないことを確認する
- 長期平均は Earth の現世値に厳密一致させる必要はないが、全海洋化または全大陸化へ単調崩壊してはならない
- **10 percentage points** は学術的な閾値ではなく、短期の崩壊や回帰を検出するための運用上の目安として扱う

注:

- ただし、plate 単位ではなく **地殻種別単位** で定義する
- 学術的に安定なのは「プレート面積比」ではなく、`continental_crust_fraction` / `oceanic_crust_fraction` のような地殻面積比である

#### Q-2: `crust_type` 条件付き hypsometry 分離

指標:

- `crust_conditioned_hypsometry_separation`

定義:

- 全球分布を 1 本で見るのではなく、`crust_type == Continental` と `crust_type == Oceanic` に分けて高度分布を比較する
- sea level を基準にしてもよいが、主判定は「2 分布の分離」に置く
- 評価量は平均差、中央値差、Wasserstein 距離、重なり率、分離指数などから選ぶ

判定:

- Continental の代表高度が Oceanic の代表高度より安定して高い
- 2 分布の重なりが極端に大きくない
- 片方の分布がほぼ消滅していない

推奨実装メモ:

- Earth との比較を意識するなら、全球 mean からの二峰判定より `crust_type` 条件付きの分離指標を優先する

#### Q-3: 海洋地殻の age-depth 一貫性

指標:

- `oceanic_age_depth_consistency`

定義:

- 海洋地殻セルのみを対象とする
- 可能なら深さは geoid/sea level 起点、難しければ海洋地殻集合内で正規化した相対深さを使う
- 年齢は `vertex_age_norm` ではなく、できるだけ物理時間に対応づいた内部 `age` を使う
- 年齢ビンごとの中央値深度系列を求める

判定:

- `age` と深さの Spearman 相関が正である
- 年齢四分位ごとの中央値で、古い海洋地殻ほど低い
- 可能なら Earth の age-depth relation あるいは plate-cooling curve に対する RMSE / 傾き誤差で比較する

意図:

- 海面変動があっても、若い海洋地殻が相対的に浅く、古い海洋地殻が相対的に沈降する一次傾向を確認する
- 単なる相関だけでなく、Earth の経験曲線から極端に逸脱していないことをみる

#### Q-4: 海嶺からの年齢勾配

指標:

- `ridge_age_gradient_consistency`

定義:

- Ridge 境界近傍の海洋地殻を起点に、海洋地殻セルの ridge からの距離と age の関係をみる
- 可能なら ridge 両側で別々に評価する

判定:

- ridge 近傍の海洋地殻年齢が系統的に若い
- ridge から離れるほど年齢中央値が増加する
- 若年地殻が海嶺からランダムに散乱していない

意図:

- 「最も若い oceanic crust は海嶺にある」という海洋底拡大の一次特徴を直接確認する

### 補助診断（海面依存）

次の量はログとしては有用だが、Geology の主判定には使わない。

- `land_ratio`
- 海面上セル比 / 海面下セル比
- 海岸線長
- 浅海棚面積

これらは、地質の破綻ではなく sea level 変動で大きく動きうるためである。

### 運用ルール

- まず前提ゲートを確認する
- 次に Q-1, Q-2, Q-3, Q-4 を確認する
- 主評価が通った場合のみ、WC の手動観察へ進む
- `land_ratio` のみ悪化し、Q-1, Q-2, Q-3, Q-4 が維持されている場合は、まず sea level 変動の影響を疑う
- 前提ゲート未通過の場合、WC 判定をしても「tectonics の妥当性」ではなく「現実装の内部整合性」しか言えない

---

## ウィルソンサイクル定量評価

**tick 換算の基準: 1 tick = 500 万年**

長期実行（WC-core・WC-1 は目安 100 tick = 5 億年、WC-3 は目安 10〜50 tick = 5000 万〜2 億 5000 万年）における挙動を、可能な限り basin-scale の定量指標で確認する。
ここでの主判定は、モデル内部の仮説変数ではなく、**海洋盆の生成・拡大・消費・閉鎖・衝突** という Earth で観測可能な一次特徴に置く。

### 基本方針

- 主判定は **basin lifecycle** を使う
- `mantle_heat`、`plume_threshold`、`rollback_fraction` は主判定に使わず、必要なら補助診断に下げる
- 超大陸の集合・分裂は Wilson cycle の長期的表れとして記録してよいが、主判定の必須条件にはしない
- 背弧盆形成は一部の沈み込み系でのみ期待されるため、主判定ではなく拡張評価に置く
- `PassiveMargin -> Subduction` の直接遷移は現実 Earth でも拘束が弱いため、必須イベントとして要求しない

### 観察前の前提チェック

WC 判定の前に、次を短く確認する。

- 大域バランス: 全海洋/全大陸への崩壊がない
- 境界分類の破綻兆候: 収束・発散・衝突の分類が長時間矛盾したまま固定されていない
- プレート連結性: 広域で不自然な孤立プレート増殖がない
- 境界形状: 境界断裂・孤立 edge・高頻度ジグザグが支配的でない

上記に重大な破綻がある場合、WC 判定は保留し、先に実装異常の切り分けを行う。

### WC-core: 海洋盆ライフサイクル

Wilson cycle の主判定は、個別の ocean basin が「開く」「成熟する」「閉じる」順序を持つかで行う。
評価単位は全球平均ではなく **basin 単位** とする。

#### 調べる

少なくとも次の 6 段階を確認する。

1. continental crust 内で rift が局在する
2. 連続した新生 oceanic crust corridor が形成される
3. ridge を中心に海洋地殻年齢が若い側から古い側へ配列する
4. ocean basin 面積が増加する時期がある
5. oceanic crust が優先的に消費される時期がある
6. basin 縮小の終末相で collision 境界または大陸接合帯が形成される

#### 指標

- `rift_localization_score`
- `connected_ocean_birth`
- `ridge_age_symmetry`
- `ocean_basin_area_trend`
- `oceanic_consumption_bias`
- `terminal_collision_signal`

#### 定義

`rift_localization_score`

- continental crust 内で、引張応力と `thickness` 低下が空間的に集中している度合い
- 「広く薄く引き延ばされる」のではなく、rift 帯が局在していることをみる

`connected_ocean_birth`

- rift 帯に沿って、連続した新生 `Oceanic` 地殻の corridor が形成されたか
- 単発の oceanic patch や一時的な浸水は数えない

`ridge_age_symmetry`

- ridge 両側で海洋地殻年齢が若い側から古い側へ増加しているか
- 左右非対称が極端すぎず、海洋底拡大の一次特徴を保っているかをみる

`ocean_basin_area_trend`

- basin 面積時系列に opening 相と closing 相の両方があるか
- 単なる短周期振動ではなく、持続的な増加相・減少相を区別する

`oceanic_consumption_bias`

- 消失または収束で失われる地殻が、continental crust より oceanic crust に偏っているか
- basin closure が「海洋盆の消費」として起きていることを確認する

`terminal_collision_signal`

- basin 消滅の終末相で `Collision` 境界、または旧 oceanic corridor を挟む大陸地殻の接合と厚化・高標高帯が現れるか
- basin 消滅後も永続的な海洋 corridor が残るなら未達とする

#### 判定

- 少なくとも 1 つの basin で `connected_ocean_birth` が成立する
- 同 basin で `ridge_age_symmetry` と `Q-3 oceanic_age_depth_consistency` が維持される
- `ocean_basin_area_trend` に opening 相と closing 相の両方がある
- closing 相で `oceanic_consumption_bias` が正である
- 終末相で `terminal_collision_signal` がある

満たした場合、WC-core は PASS とする。

### WC-1: 大陸集合イベント

超大陸の集合・分裂は Wilson cycle の長期表現のひとつだが、単独 basin の開閉より強い条件である。
したがって、主判定ではなく拡張評価として扱う。

確認ポイント:

- 大陸地殻の連結成分が粗視化して集約する時期があるか
- 大規模 `Collision` 境界または接合帯が形成されるか
- その後、再び rift が局在し新しい ocean basin 候補が生まれるか

補助指標の例:

- `continental_aggregation_index`
- `major_collision_count`
- `post_collision_rift_reuse`

注:

- `mantle_heat` や `plume_threshold` は breakup 機構の内部仮説であり、Earth 類似性の主証拠には使わない
- WC-1 未観測は、直ちに FAIL ではなく「観測窓不足」または「長期サイクル未到達」の可能性を含む

### WC-2: basin lifecycle の順序性

WC-core の詳細観察として、各 basin でフェーズ順序が崩れていないかを確認する。
ここでは内部 phase label ではなく、観測可能な地殻・年齢・境界配置から判定する。

| フェーズ  | 確認する物理量の状態                                                                   |
| --------- | -------------------------------------------------------------------------------------- |
| pre-rift  | `stress > 0`（引張）かつ `thickness` が減少傾向にある                                  |
| rift 進行 | `thickness` 低下が局在し、連続した rift 帯を形成する                                   |
| 海洋誕生  | 発散境界沿いに新生 oceanic crust corridor が形成される。浸水そのものは必要条件ではない |
| 海洋拡大  | ridge から両側に若い海洋地殻が付加され、時間とともに `age` が増加する                  |
| 海洋消費  | 収束帯で oceanic crust が preferentially に失われ、basin 面積が減少する                |
| 終末衝突  | basin の消滅に対応して `Collision` 境界または大陸接合帯が形成される                    |

判定上の注意:

- `PassiveMargin -> Subduction` の直接遷移は Earth でも一般化しにくいため、必須条件にしない
- basin closure の証拠は「海洋地殻消費」と「最終衝突シグナル」の組で評価する
- 順序が頻繁に逆転する場合は、phase の実在ではなくノイズの可能性を疑う

### WC-3: 島弧・背弧の形成

沈み込み系の realism をみる拡張評価であり、Wilson cycle の主判定ではない。
特に背弧盆形成は一部の沈み込み設定に限られるため、未観測でも直ちに FAIL にしない。

沈み込みに伴う火山・地形の変化を確認する。WC-core より短いタイムスケールで観察できる。

確認ポイント:

- Subduction 境界の大陸側に `arc_volcanism > 0` のセルが分布するか
- trench から arc までの距離が極端に乱れず、上盤側に偏在するか
- `rollback_fraction > rollback_threshold` の edge がある場合に限り、背弧側で引張応力と `backarc_volcanism` が共起するか
- 背弧側で伸張と盆地形状が出てもよいが、未観測は optional 扱いとする

推奨指標:

- `arc_on_overriding_plate_ratio`
- `arc_trench_offset_stability`
- `backarc_optional_score`

---

## 評価記録の形式

各チェック実行後に、以下の形式で結果を記録する。

```
実行日時: YYYY-MM-DD
seed: <使用した seed>
params: <使用した params 名またはハッシュ>
ticks または実行内容:

[定量評価]
前提ゲート crust_type independence: PASS / FAIL / 要確認 → コメント
Q-1 continental_crust_fraction / oceanic_crust_fraction: PASS / FAIL / 要確認 → コメント
Q-2 crust_conditioned_hypsometry_separation: PASS / FAIL / 要確認 → コメント
Q-3 oceanic_age_depth_consistency: PASS / FAIL / 要確認 → コメント
Q-4 ridge_age_gradient_consistency: PASS / FAIL / 要確認 → コメント
補助診断 land_ratio: 値 / 変動幅 → コメント

[前提チェック]
大域バランス: OK / 異常 → コメント
境界分類の破綻兆候: なし / あり → コメント
プレート連結性: 問題なし / 問題あり → コメント
境界形状: 許容 / 要調査 → コメント

[ウィルソンサイクル]
WC-core basin lifecycle: PASS / FAIL / 要確認 → コメント
opening evidence: あり / なし / 不明 → コメント
mature spreading evidence: あり / なし / 不明 → コメント
closure evidence: あり / なし / 不明 → コメント
terminal collision evidence: あり / なし / 不明 → コメント
WC-1 continental aggregation: 観察できた / 観察できなかった / 不明 → コメント
WC-3 arc realism: PASS / FAIL / 要確認 → コメント
WC-3 backarc realism: 観察できた / 観察できなかった / N/A → コメント

所感・次のアクション:
```

---

## 既知の限界（検証対象外）

以下はモデル設計上の限界であり、「観察できなかった」と記録しても直ちに不具合扱いしない。

- プレート形状の現実との定量的一致（大きさ・形・個数）
- 沈み込み傾斜角の定量的再現
- 超大陸サイクル周期の定量的一致（現実の約 5 億年との比較）
- 海洋熱沈降の絶対値精度
- 受動縁辺での沈み込み開始の再現性そのもの
- 背弧盆形成の普遍的再現
- 海面変動そのものの妥当性評価（本書では地質構造の評価を優先する）

---

## 実装再構成への示唆

現実装または今後の再構成で、少なくとも次を満たさない限り、本書の主評価を pass/fail に使ってはならない。

- `crust_type` を `height` から初期化・再導出しない
- 海洋地殻年齢 `age` を高さ由来ではなく、ridge 生成時刻から積分する
- ridge / rift / subduction の境界識別と、`age`・`crust_type` の更新規則を整合させる
- `thermal_subsidence` を age-depth 評価と整合する形で有効化する

上記が未達の段階では、本書は「Earth 類似度の判定基準」ではなく、「将来の geology model が満たすべき benchmark 仕様」として扱う。

---

## 参照

- `docs/concepts/overview.md`
- `docs/reference/architecture/data_model.md`
- `docs/reference/architecture/module_boundaries.md`
- `docs/operations/bench/README.md`
- `docs/operations/bench/climate/solo.md`
- `docs/operations/bench/hydrology/solo.md`
- `docs/operations/bench/ecology/solo.md`
- `docs/operations/test.md`（回帰テスト・日常的な壊れ確認）
- `docs/reference/modules/geology.md`（Plate モジュール仕様）
- Burke, K. (2011). Plate Tectonics, the Wilson Cycle, and Mantle Plumes: Geodynamics from the Top.
- Cawood, P. A., & Hawkesworth, C. J. (2018). Continental crustal volume, thickness and area, and their geodynamic implications.
- Zhong, S., & Li, Z.-X. (2021). Subduction initiation and the onset of plate tectonics.
- Pedersen, V. K. et al. (2024). Earth's hypsometry and what it tells us about global sea level.
- Seton, M. et al. (2020). A Global Data Set of Present-Day Oceanic Crustal Age and Seafloor Spreading Parameters.
- Artemieva, I. M. (2023). Back-arc basins: A global view from geophysical synthesis and analysis.
- NOAA Ocean Explorer. Mid-Ocean Ridge Activity.
