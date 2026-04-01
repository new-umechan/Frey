# Geology 検証設計

`benchmark.md` の Plate 節（ベンチマーク対象外）を補完する、Plate 専用の検証ドキュメント。
本書は、重い実データ一致ベンチではなく、Earth 類似の物理整合性をみる軽量な定量評価と、ウィルソンサイクル（WC）の定性評価を組み合わせて運用する。

---

## 位置づけ

Geology は Climate/Hydrology/Ecology のような実測値との直接比較ベンチマークの対象外である。
代わりに、以下で妥当性を担保する。

| 種別 | 目的 | 実行タイミング |
|---|---|---|
| **自動検証（コード）** | 実装破綻を早期に検知する | `cargo test` / `debug_assert!` |
| **定量評価（軽量）** | 海面変動に依らない地質構造の整合性を確認する | モデル・主要パラメータ変更時 |
| **WC 定性評価（手動）** | 長期挙動が「それらしいか」を判断する | 定量評価通過後 |

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

| 案 | 内容 | 採否 |
|---|---|---|
| A | `land_ratio` や海面上セル比で評価する | 不採用 |
| B | `crust_type` を基準に地殻構造を評価し、海面依存量は別管理する | 条件付き採用 |
| C | 全球相対高度分布のみで hypsometry をみる | 不採用 |
| D | `crust_type` 条件付き高度分布、海洋 age-depth、ridge age gradient を評価する | 採用 |

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

#### Q-1: 大陸/海洋地殻面積比

指標:

- `continental_crust_fraction`

定義:

- `crust_type == Continental` のセル面積比
- `land_ratio` は使わない
- ただし `crust_type` が provenance を表す内部状態であることを前提とする

判定:

- 100 tick 観測窓で変動幅が **10 percentage points** を超えない
- Earth preset では平均値が極端に片寄らないことを確認する
- 長期平均は Earth の現世値に厳密一致させる必要はないが、全海洋化または全大陸化へ単調崩壊してはならない

注:

- 「大陸プレートと海洋プレートの面積割合が 10% 以上変動しない」という案は、この Q-1 に吸収する
- ただし、plate 単位ではなく **地殻種別単位** で定義する

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

## ウィルソンサイクル定性評価

**tick 換算の基準: 1 tick = 500 万年**

長期実行（WC-1・WC-2 は目安 100 tick = 5 億年、WC-3 は目安 10〜50 tick = 5000 万〜2 億 5000 万年）における挙動を目視で確認する。
実データとの定量一致ではなく、「意味のある地質サイクルが再現されているか」の定性評価とする。

### 観察前の前提チェック

WC 観察の前に、次を短く確認する。

- 大域バランス: 全海洋/全大陸への崩壊がない
- 境界分類の破綻兆候: 収束・発散・衝突の分類が長時間矛盾したまま固定されていない
- プレート連結性: 広域で不自然な孤立プレート増殖がない
- 境界形状: 境界断裂・孤立 edge・高頻度ジグザグが支配的でない

上記に重大な破綻がある場合、WC 判定は保留し、先に実装異常の切り分けを行う。

### WC-1: 超大陸の集合と分裂

確認ポイント:

- 大陸地殻が一箇所に集積するフェーズが発生するか
- 集積した大陸地殻の下で `mantle_heat` が上昇するか（大陸地殻の放熱率が低いため）
- `mantle_heat > plume_threshold` となり uplift force が発生するか
- 大陸が分裂し、後続フェーズへ移行するか

### WC-2: 海洋の開閉

物理量の条件ベースで各フェーズの発生を確認する。

| フェーズ | 確認する物理量の状態 |
|---|---|
| pre-rift | `stress > 0`（引張）かつ `thickness` が減少傾向にある |
| rift 進行 | `thickness` が閾値以下まで減少している |
| 海洋誕生 | 発散境界で新生 oceanic crust が生成される。浸水そのものは必要条件ではない |
| 海洋拡大 | 海嶺から両側に若い海洋地殻が付加され、時間とともに `age` と `density` が増加する |
| 沈み込み開始 | 高密度化した海洋地殻で PassiveMargin から沈み込みへの移行が起きる |
| 海洋消滅 | 海洋地殻がすべて沈み込み、Collision 境界へ移行する |

**NOTE: 沈み込み開始条件について**
現行実装では、沈み込み開始は正規化済みの `age_norm > subduction_initiation_threshold` かつ `density_norm > subduction_density_threshold` を満たす場合、または `age_norm * subduction_age_coupling + density_norm > 1.0` を満たす場合に発生する。
評価時はこの実装条件に従って「PassiveMargin から Subduction への移行」を判定する。

### WC-3: 島弧・背弧の形成

沈み込みに伴う火山・地形の変化を確認する。WC-1・WC-2 より短いタイムスケールで観察できる。

確認ポイント:

- Subduction 境界の大陸側に `arc_volcanism > 0` のセルが分布するか
- `rollback_fraction > rollback_threshold` の edge で背弧側に引張応力が発生するか
- 背弧側で `backarc_volcanism > 0` が発生し、地形的な盆地形状が形成されるか

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
Q-1 continental_crust_fraction: PASS / FAIL / 要確認 → コメント
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
WC-1: 観察できた / 観察できなかった / 不明 → コメント
WC-2: 各フェーズの達成状況 → コメント
WC-3: 観察できた / 観察できなかった / 不明 → コメント

所感・次のアクション:
```

---

## 既知の限界（検証対象外）

以下はモデル設計上の限界であり、「観察できなかった」と記録しても直ちに不具合扱いしない。

- プレート形状の現実との定量的一致（大きさ・形・個数）
- 沈み込み傾斜角の定量的再現
- 超大陸サイクル周期の定量的一致（現実の約 5 億年との比較）
- 海洋熱沈降の絶対値精度
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

- `docs/architecture/overview.md`
- `docs/architecture/data_model.md`
- `docs/architecture/module_boundaries.md`
- `docs/manage/bench/` 配下（Climate/Hydrology/Ecology のベンチ詳細）
- `docs/manage/test.md`（回帰テスト・日常的な壊れ確認）
- `docs/modules/geology.md`（Plate モジュール仕様）
- Cawood, P. A., & Hawkesworth, C. J. (2018). Continental crustal volume, thickness and area, and their geodynamic implications.
- Pedersen, V. K. et al. (2024). Earth's hypsometry and what it tells us about global sea level.
- Seton, M. et al. (2020). A Global Data Set of Present-Day Oceanic Crustal Age and Seafloor Spreading Parameters.
- NOAA Ocean Explorer. Mid-Ocean Ridge Activity.
