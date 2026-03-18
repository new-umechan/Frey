# 時代とtick制御

## 目的

この文書は、時代、tick、予算、時代遷移を定義する。
ここで扱うのは時間制御であり、各モジュールの責務詳細ではない。

## Tick

tickは単なるカウンタではなく、そのtickが表す実時間の密度を持つ。

```rust
struct Tick {
    real_years: f32,
    scale: EpochScale,
    budgets: SubsystemBudgets,
}
```

- `real_years`
  - 1tickが表す実世界年数
- `scale`
  - 現在の時代
- `budgets`
  - 各モジュールの更新回数近似

## 時代一覧

| 時代 | 主対象 | 1tickの意味 | 重点 |
| --- | --- | --- | --- |
| 地殻形成期 | `Geology` | 500万年 | プレート運動、境界活動、海陸骨格 |
| 環境形成期 | `Climate` / `Hydrology` | 1万年 | 降水、流出、流路、流量、侵食、堆積 |
| 生命誕生期 | `Ecology` / `Domesticates` / `Subsistence` | 1000年 | 可住性、生産性、作物・家畜分布、生業成立 |
| 文明成立期 | `Population` / `Settlement` / `Polity` | 100年 | 定住、都市化、初期国家形成 |
| 歴史展開期 | `Conflict`（+Tier 2） | 1年 | 国家競合、戦争、交易、技術変化 |

時代は、モジュールを開始停止する排他的な段階ではない。
どのモジュールをどれだけ強く更新するかを決める時間スケールである。

## 状態の有効化タイミング

時代に応じて、必要な状態群を順次有効化する。

| 時代 | 有効な状態 |
| --- | --- |
| 地殻形成期 | `Geology` のみ |
| 環境形成期 | `Climate` と `Hydrology` を初回有効化 |
| 生命誕生期 | `Ecology` / `Domesticates` / `Subsistence` を初回有効化 |
| 文明成立期 | `Population` / `Settlement` / `Polity` を初回有効化 |
| 歴史展開期 | 既存状態をすべて保持したまま運用 |

地殻形成期では、`Climate` と `Ecology` がまだ有効でなくても `Geology` が未初期化値を読まないようにする。
この時期の既定入力は次の通り。

- 降水は、地殻形成期向けの簡易な初期降水分布を使う
- 流量は 0 とする
- 流域植生は なし とする

## 予算配分

`SubsystemBudgets` は、各モジュールに与える内部更新回数の近似である。
初版では整数回数として扱う。

| 時代 | `Geology` | `Climate` | `Hydrology` | `Ecology` | `Domesticates` | `Subsistence` | `Population` | `Settlement` | `Polity` | `Conflict` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 地殻形成期 | 高 | 低 | なし | なし | なし | なし | なし | なし | なし | なし |
| 環境形成期 | 中 | 高 | 高 | 低 | なし | なし | なし | なし | なし | なし |
| 生命誕生期 | 低 | 中 | 中 | 高 | 中 | 中 | 低 | なし | なし | なし |
| 文明成立期 | 低 | 低 | 中 | 中 | 中 | 高 | 高 | 高 | 高 | 低 |
| 歴史展開期 | 低 | 低 | 低 | 低 | 低 | 中 | 高 | 高 | 高 | 高 |

歴史展開期のように活動量が低いモジュールはスキップ可能とする。
スキップ条件の閾値は後続バージョンで定義する。

## 時代遷移

時代遷移は固定tick数ではなく、状態条件で決める。
これにより、惑星ごとに時代の長さが変わる。

```rust
type EpochGuard = fn(&WorldState) -> bool;

const EPOCH_TRANSITIONS: &[(Epoch, EpochGuard)] = &[
    (Epoch::Geological, sea_land_ratio_stable),
    (Epoch::Climate, river_network_formed),
    (Epoch::Ecology, habitable_area_above_threshold),
    (Epoch::Society, has_settlement),
];
```

上の擬似コードは概念を示したものであり、実際の閾値や判定式は未確定である。

## 更新ループ

1tickの標準順序は次の通り。

1. tick開始時に `FeedbackQueue` の内容を `World State` と `Graph State` に適用する
2. `Geology`
3. `Climate`
4. `Hydrology`
5. `Ecology`
6. `Domesticates`
7. `Subsistence`
8. `Population`
9. `Settlement`
10. `Polity`
11. `Conflict`
12. 各モジュールが次tick向けの影響を `FeedbackQueue` に格納する
13. 時代遷移判定

補足:

- 同一tick内の依存はDAGで保証する
- `FeedbackQueue` への格納は tick N の末尾で行う
- `FeedbackQueue` の適用は tick N+1 の開始時に行う
- 同一tick内で逆向きの即時反映は行わない

## 将来の拡張候補

- モジュールごとの内部時間幅
- 計算コスト上限
- 活動量に応じたスキップ率
- 海流、火山など複数スケール現象の扱い
