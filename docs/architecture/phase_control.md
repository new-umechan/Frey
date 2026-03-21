# 時代とtick制御

## 目的

この文書は、時代、tick、予算、時代遷移、更新順序を定義する。
ここで扱うのは時間制御と実行制御であり、各モジュールの責務詳細ではない。
データ構造の定義は `docs/architecture/data_model.md`、各Systemの読み書き境界は `docs/architecture/module_boundaries.md` を参照する。

## Tick

tickは単なるカウンタではなく、そのtickが表す実時間の密度を持つ。

```rust
struct Clock {
    tick:    Tick,
    epoch:   Epoch,
    budgets: SubsystemBudgets,
}

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
  - 各Moduleの更新回数近似

## 時代一覧

| 時代 | 主対象 | 1tickの意味 | 重点 |
| --- | --- | --- | --- |
| 地殻形成期 | `Geology` | 500万年 | プレート運動、境界活動、海陸骨格 |
| 環境形成期 | `Climate` / `Hydrology` | 1万年 | 降水、流出、流路、流量、侵食、堆積 |
| 生命誕生期 | `Ecology` / `Domesticates` / `Subsistence` | 1000年 | 可住性、生産性、作物・家畜分布、生業成立 |
| 文明成立期 | `Population` / `Settlement` / `Polity` | 100年 | 定住、都市化、初期国家形成 |
| 歴史展開期 | `Conflict`（+Tier 2） | 1年 | 国家競合、戦争、交易、技術変化 |

時代は、Moduleを開始停止する排他的な段階ではない。
どのModuleをどれだけ強く更新するかを決める時間スケールである。

## 状態の有効化タイミング

時代に応じて、必要な状態群を順次有効化する。

| 時代 | 有効な状態 |
| --- | --- |
| 地殻形成期 | `Geology` を主とし、`Climate` は簡易・低頻度で運用 |
| 環境形成期 | `Climate` と `Hydrology` を初回有効化 |
| 生命誕生期 | `Ecology` / `Domesticates` / `Subsistence` を初回有効化 |
| 文明成立期 | `Population` / `Settlement` / `Polity` を初回有効化 |
| 歴史展開期 | 既存状態をすべて保持したまま運用 |

地殻形成期では、`Climate` は簡易モードとして動作し、`Ecology` は未有効である。
この時期でも `Geology` が未初期化値を読まないようにする。
この時期の既定入力は次の通り。

- 降水は、地殻形成期向けの簡易な初期降水分布を使う
- 流量は 0 とする
- 流域植生は なし とする

## 予算配分

`SubsystemBudgets` は、各Moduleに与える内部更新回数の近似である。
初版では整数回数として扱う。

| 時代 | `Geology` | `Climate` | `Hydrology` | `Ecology` | `Domesticates` | `Subsistence` | `Population` | `Settlement` | `Polity` | `Conflict` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 地殻形成期 | 高 | 低 | なし | なし | なし | なし | なし | なし | なし | なし |
| 環境形成期 | 中 | 高 | 高 | 低 | なし | なし | なし | なし | なし | なし |
| 生命誕生期 | 低 | 中 | 中 | 高 | 中 | 中 | 低 | なし | なし | なし |
| 文明成立期 | 低 | 低 | 中 | 中 | 中 | 高 | 高 | 高 | 高 | 低 |
| 歴史展開期 | 低 | 低 | 低 | 低 | 低 | 中 | 高 | 高 | 高 | 高 |

歴史展開期のように活動量が低いModuleはスキップ可能とする。
スキップ条件の閾値は後続バージョンで定義する。

実際の更新は `System` 単位で行う。
`ExecSystem` は時代・状態・予算を参照して、各Module内で実行する `System` を選択する。

```rust
// 擬似型: Moduleごとの実行対象System列
type SystemPlan = HashMap<ModuleId, Vec<SystemId>>;
```

## 時代遷移

時代遷移は状態条件で決めるが、デバッグと安定化のために `min_ticks`・`max_ticks` のガードを加える。

```rust
// WorldContext は CellStore / hecs::World / Clock への参照束を表す擬似型
type EpochGuard = fn(&WorldContext) -> bool;

struct EpochTransition {
    condition: EpochGuard,
    min_ticks: u32,           // 条件達成後も最短この tick 数は保持する（安定化）
    max_ticks: Option<u32>,   // 条件未達でも強制遷移するtick数上限（デバッグ・異常系）
}

const EPOCH_TRANSITIONS: &[(Epoch, EpochTransition)] = &[
    (Epoch::Geological, EpochTransition { condition: sea_land_ratio_stable,    min_ticks: 3, max_ticks: Some(200) }),
    (Epoch::Climate,    EpochTransition { condition: river_network_formed,     min_ticks: 3, max_ticks: Some(200) }),
    (Epoch::Ecology,    EpochTransition { condition: habitable_area_above_threshold, min_ticks: 3, max_ticks: Some(500) }),
    (Epoch::Society,    EpochTransition { condition: has_settlement,           min_ticks: 3, max_ticks: None       }),
];
```

具体的な閾値・判定式・数値は未確定であり、実装時に調整する。
`max_ticks: None` は強制遷移なし（歴史展開期への移行は状態条件のみで決める）を意味する。

## 更新ループ

1tickの標準順序は次の通り。

1. tick開始時に `ExecSystem` が `FeedbackQueue` の内容を一括で `CellStore` と `hecs::World` に適用する
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

## 並列化と再現性

WASMではシングルスレッドで動作するため、現時点では並列化を行わない。

将来的に並列化を導入する場合は、処理順序が結果に影響しないModuleのみを対象にする。
セル間で値を読み合う計算（拡散・流路計算など）はシングルスレッド実行を維持し、
同一seed・同一パラメータでの再現性を保証する。
