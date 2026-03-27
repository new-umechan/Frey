# Geology 検証設計

`benchmark.md` のPlate節（ベンチマーク対象外）を補完する、Plate専用の検証ドキュメント。
定量比較ではなく「正常に動いているか」を判定するための道具として使う。

---

## 位置づけ

Geologyはベンチマーク（Climate/Hydrology/Ecologyの定量評価）の対象外である。
代わりに以下の2軸で検証する。

| 軸 | 目的 | 実行タイミング |
|---|---|---|
| **サニティチェック** | 実装が壊れていないかを確認する | 実装変更のたびに手動実行 |
| **ウィルソンサイクル定性評価** | 長期挙動が「それらしいか」を確認する | モデル・パラメータ変更時に手動実行 |

---

## サニティチェック

### SC-0：大域バランス

全体の海陸比・地殻組成比が極端な値になっていないかを確認する。

| 確認項目 | 期待値 | 備考 |
|---|---|---|
| 海面上セル比率 | 実装時に調整 | 極端値（全海洋・全大陸）を検出できればよい |
| 大陸地殻セル比率 | 実装時に調整 | 同上 |
| 長期実行での崩壊 | 全セルが同一 `crust_type` にならない | 数百tick後に確認 |

長期実行での崩壊（全セルが `Oceanic` または `Continental` に収束する）は、モデルの根本的な異常として扱う。

### SC-1：配列整合性

`GeologyOutput` および `GeologyDynamicsState` の各配列が頂点数と一致しているかを確認する。

| 確認項目 | 期待値 |
|---|---|
| `height.len()` | 頂点数と一致 |
| `plate_id.len()` | 頂点数と一致 |
| `river_flux.len()` | 頂点数と一致 |
| `river_next.len()` | 頂点数と一致 |
| `mantle_heat.len()` | 頂点数と一致 |
| `vertex_states.len()` | 頂点数と一致 |

### SC-2：値域チェック

各フィールドの値が許容範囲内に収まっているかを確認する。NaN・Infはすべてのフィールドで不正とする。

| フィールド | 許容範囲 | 備考 |
|---|---|---|
| `height` | `[-1.5, 1.5]` | 内部表現の上下限 |
| `plate_id` | `[0, plate数-1]` | 孤立頂点は別途記録 |
| `river_next` | `-1` または `[0, 頂点数-1]` | ループは終端化または修正済みであること |
| `thickness` | `> 0` | 負値は不正 |
| `density` | `> 0` | 負値は不正 |
| `age` | `>= 0` | 負値は不正 |
| `mantle_heat` | `[0, 1]` | 正規化済み |
| `stress` | 任意（符号あり） | NaN/Infは不正 |
| `convergence_memory` | `[0, 1]` | 正規化済み |
| `rollback_fraction` | `[0, rollback_fraction_max]` | パラメータ上限を超えないこと |
| `slab_convergence_component` | 任意（符号あり） | NaN/Infは不正 |
| `slab_rollback_component` | 任意（符号あり） | NaN/Infは不正 |
| `volcanism` | `>= 0` | 負値は不正 |
| `arc_volcanism` | `>= 0` | 負値は不正 |
| `ridge_volcanism` | `>= 0` | 負値は不正 |
| `hotspot_volcanism` | `>= 0` | 負値は不正 |
| `backarc_volcanism` | `>= 0` | 負値は不正 |

**NOTE：stressの検証対象について**
`vertex_states[i].stress` はスカラー（`f32`）であり、これをSC-2の値域チェック対象とする。
`StressTensor`（`xx, yy, xy`）を別途保持する場合、その全成分についても同様にNaN/Infチェックを行う。

### SC-3：境界整合性

境界分類が相対速度と矛盾していないかを確認する。

| 確認項目 | 期待される状態 |
|---|---|
| Subduction境界 | 収束速度 > 0 かつ 少なくとも片側が海洋地殻で、沈み込み開始条件を満たす |
| Ridge/Rift境界 | 発散速度 > 0 |
| Collision境界 | 収束速度 > 0 かつ 大陸地殻同士が隣接 |
| Transform境界 | 相対速度がせん断方向に支配的 |
| PassiveMargin | 収束・発散ともに低速 |

不一致を検出した場合、初回検出から `boundary_reclassify_interval` tick以内に解消されることを確認する。
`boundary_reclassify_interval` tick経過後も不一致が残る場合はエラーとして記録する。

### SC-4：河川ループ検出

`river_next` がループを形成していないことを確認する。

- 終端（`river_next == -1`）に到達しない頂点が存在する場合はエラーとして記録する

### SC-5：プレート連結性

各 `plate_id` が連結した領域を形成しているかを確認する。

- 孤立した単一セルのプレートを検出し、数を記録する
- 境界再分類直後の一時的な孤立は許容するが、`boundary_reclassify_interval` tick以内の解消を確認する

### SC-6：決定性チェック

同一 seed + params + 更新スケジュールで2回実行し、出力が一致するかを確認する。
比較はε = 1e-5の許容誤差付きとする（f32の演算誤差・`convergence_memory`の平滑化誤差の積み上がりを考慮）。

比較対象：

| 対象 | フィールド |
|---|---|
| 公開出力 | `height`, `plate_id`, `river_flux`, `river_next` |
| 頂点地殻状態 | `thickness`, `density`, `age`, `stress`, `temperature`, `rigidity`, `arc_volcanism`, `ridge_volcanism`, `hotspot_volcanism`, `backarc_volcanism` |
| マントル熱場 | `mantle_heat` |
| 境界状態 | `convergence_memory`, `slab_convergence_component`, `slab_rollback_component` |

不一致があればバグとして扱う。

### SC-7：シリアライズ検証

チェックポイント機構が正しく動作するかを確認する。

**検証1：snapshot一致**
`World` 全体を serialize → deserialize 後に `world.state.geology` と `world.runtime.geology_dynamics` を比較し、元のsnapshotおよび内部状態と一致するかを確認する。

**検証2：step後一致**
serialize → deserialize 後に `update_geology(world, budget)` を実行し、元の状態から継続した場合と出力が一致するかを確認する。
比較対象と許容誤差はSC-6に準じる。

### SC-8：境界形状チェック

プレート境界が破綻した形状になっていないかを目視および指標で確認する。

| 確認項目 | 検出対象 |
|---|---|
| 境界の断裂 | 同一境界タイプのedgeが連続していない箇所 |
| 境界の孤立 | 隣接edgeを持たない孤立したboundary edge |
| ノイズ的ジグザグ | 隣接edge間で境界タイプが高頻度に交互変化している箇所 |

`BoundaryDynamicsState` はedgeごとの境界タイプを永続保持しないため、SC-8では検証時に境界edgeと境界タイプを再計算して評価する。
断裂・孤立の件数を記録する。ジグザグは目視確認を基本とし、「境界タイプの変化回数 / 境界edge総数」を補助指標として記録する。

---

## ウィルソンサイクル定性評価

**tick換算の基準：1 tick = 500万年**

長期実行（WC-1・WC-2は目安100tick = 5億年、WC-3は目安10〜50tick = 5000万〜2億5000万年）における挙動を目視で確認する。
実データとの比較ではなく、「それらしい挙動が起きているか」の定性評価とする。

### WC-1：超大陸の集合と分裂

確認ポイント：

- 大陸地殻が一箇所に集積するフェーズが発生するか
- 集積した大陸地殻の下で `mantle_heat` が上昇するか（大陸地殻の放熱率が低いため）
- `mantle_heat > plume_threshold` となりuplift_forceが発生するか
- 大陸が分裂し、後続フェーズへ移行するか

### WC-2：海洋の開閉

物理量の条件ベースで各フェーズの発生を確認する。

| フェーズ | 確認する物理量の状態 |
|---|---|
| pre-rift | `stress > 0`（引張）かつ `thickness` が減少傾向にある |
| rift進行 | `thickness` が閾値以下まで減少している |
| 海洋誕生 | `crust_type` が `Continental → Oceanic` へ変化する（標高が海面下に達したとき） |
| 海洋拡大 | 海嶺から両側に `age = 0` のセルが付加され、時間とともに `age` と `density` が増加する |
| 沈み込み開始 | 高密度化した海洋地殻でPassiveMarginから沈み込みへの移行が起きる |
| 海洋消滅 | 海洋地殻がすべて沈み込み、Collision境界へ移行する |

**NOTE：沈み込み開始条件について**
現行実装では、沈み込み開始は正規化済みの `age_norm > subduction_initiation_threshold` かつ `density_norm > subduction_density_threshold` を満たす場合、または `age_norm * subduction_age_coupling + density_norm > 1.0` を満たす場合に発生する。
評価時はこの実装条件に従って「PassiveMarginからSubductionへの移行」を判定する。

### WC-3：島弧・背弧の形成

沈み込みに伴う火山・地形の変化を確認する。WC-1・WC-2より短いタイムスケールで観察できる。

確認ポイント：

- Subduction境界の大陸側に `arc_volcanism > 0` のセルが分布するか
- `rollback_fraction > rollback_threshold` のedgeで背弧側に引張応力が発生するか
- 背弧側で `backarc_volcanism > 0` が発生し、地形的な盆地形状が形成されるか

---

## 評価記録の形式

各チェック実行後に、以下の形式で結果を記録する。

```
実行日時: YYYY-MM-DD
seed: <使用したseed>
params: <使用したparams名またはハッシュ>
ticksまたは実行内容:

[サニティチェック]
SC-0: OK / 海陸比異常 / 長期崩壊検出 → コメント
SC-1: OK / 配列長不一致 (対象: ...)
SC-2: OK / 値域違反 N件 (対象フィールド: ...)
SC-3: OK / boundary_reclassify_interval超過エラー N件
SC-4: OK / ループ N件
SC-5: OK / 孤立プレート N件
SC-6: OK / 不一致あり (対象: ...)
SC-7: OK / snapshot不一致 / step後不一致
SC-8: OK / 断裂 N件 / 孤立edge N件 / ジグザグ率 X%

[ウィルソンサイクル]
WC-1: 観察できた / 観察できなかった / 不明 → コメント
WC-2: 各フェーズの達成状況 → コメント
WC-3: 観察できた / 観察できなかった / 不明 → コメント

所感・次のアクション:
```

---

## 既知の限界（検証対象外）

以下はモデルの設計上の限界であり、検証で「観察できなかった」と記録しても修正対象ではない。

- プレート形状の現実との定量的一致（大きさ・形・個数）
- 沈み込みの傾斜角の定量的再現
- 超大陸サイクルの周期の定量的一致（現実の約5億年との比較）
- 海洋熱沈降の絶対値精度

---

## 参照

- `docs/architecture/overview.md` ・ `docs/architecture/data_model.md` ・ `docs/architecture/module_boundaries.md`（設計正本）
- `docs/manage/bench/` 配下（Climate/Hydrology/Ecology のベンチ詳細）
- `docs/manage/test.md`（回帰テスト・日常的な壊れ確認）
- `geology.md`（Plateモジュール仕様）
