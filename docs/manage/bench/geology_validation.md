# Geology 検証設計

`benchmark.md` の Plate 節（ベンチマーク対象外）を補完する、Plate 専用の検証ドキュメント。
本書は「手動で観察する価値が高い検証」に限定し、主眼をウィルソンサイクル（WC）の定性評価に置く。

---

## 位置づけ

Geology は Climate/Hydrology/Ecology のような定量ベンチマークの対象外である。
代わりに、以下で妥当性を担保する。

| 種別 | 目的 | 実行タイミング |
|---|---|---|
| **自動検証（コード）** | 実装破綻を早期に検知する | `cargo test` / `debug_assert!` |
| **WC 定性評価（手動）** | 長期挙動が「それらしいか」を判断する | モデル・主要パラメータ変更時 |

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
| 海洋誕生 | `crust_type` が `Continental -> Oceanic` へ変化する（標高が海面下に達したとき） |
| 海洋拡大 | 海嶺から両側に `age = 0` のセルが付加され、時間とともに `age` と `density` が増加する |
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
seed: <使用したseed>
params: <使用したparams名またはハッシュ>
ticksまたは実行内容:

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

---

## 参照

- `docs/architecture/overview.md`
- `docs/architecture/data_model.md`
- `docs/architecture/module_boundaries.md`
- `docs/manage/bench/` 配下（Climate/Hydrology/Ecology のベンチ詳細）
- `docs/manage/test.md`（回帰テスト・日常的な壊れ確認）
- `geology.md`（Plate モジュール仕様）
