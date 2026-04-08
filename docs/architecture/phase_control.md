# 時代とtick制御

## 目的

この文書は、時代、tick、予算、時代遷移、更新順序を定義する。
ここで扱うのは時間制御と実行制御であり、各モジュールの責務詳細ではない。
データ構造の定義は `docs/architecture/data_model.md`、各Systemの読み書き境界は `docs/architecture/module_boundaries.md` を参照する。

## Tick

tickは単なるカウンタではなく、そのtickが表す実時間の密度を持つ。

```rust
struct Clock {
    tick:           u32,   // 累計tick数（0始まり）
    epoch:          Epoch,
    budgets:        SubsystemBudgets,
    real_target_ms: u32,   // 1tickのリアルタイム実行目標（ms）。初期値: 100
}
```

`tick` の現在値から `Epoch` は一意に決まり、`Epoch` から `real_years`（1tickが表す実世界年数）も一意に決まる。
`real_years` は `Clock` のフィールドとして持たず、必要な箇所で `epoch.real_years_per_tick()` として導出する。

`real_target_ms` はExecSystemがtick実行時間を監視してスキップ判定に使う目安値であり、シミュレーション結果には影響しない。

## 時代一覧

| 時代 | 主対象 | 開始年 | 終了年 | 1tick | tick数 | 累計tick | 重点 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 地殻形成期 | `Geology` | -45億年 | -5億年 | 500万年 | 800 | 800 | プレート運動、境界活動、海陸骨格 |
| 環境形成期 | `Climate` / `Hydrology` | -5億年 | -10万年 | 100万年 | 500 | 1,300 | 降水、流出、流路、流量、侵食、堆積 |
| 先史期 | `Ecology` / `Domesticates` / `Subsistence` | -10万年 | -5,500年 | 1,000年 | 95 | 1,395 | 可住性、生産性、作物・家畜分布、生業成立 |
| 文明成立期 | `Population` / `Settlement` / `Polity` | -5,500年 | -500年 | 100年 | 50 | 1,445 | 定住、都市化、初期国家形成 |
| 歴史展開期 | `Conflict`（+Tier 2） | -500年 | — | 1年 | 上限なし | 1,445〜 | 国家競合、戦争、交易、技術変化 |

歴史展開期の終了年は定義しない。何年まで回すかは実行時に決める。

時代は、Moduleを開始停止する排他的な段階ではない。
どのModuleをどれだけ強く更新するかを決める時間スケールである。

## 状態の有効化タイミング

時代に応じて、必要な状態群を順次有効化する。

| 時代 | 有効な状態 |
| --- | --- |
| 地殻形成期 | `Geology` を主とし、`Climate` は簡易・低頻度で運用 |
| 環境形成期 | `Climate` と `Hydrology` を初回有効化 |
| 先史期 | `Ecology` / `Domesticates` / `Subsistence` を初回有効化 |
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

| 時代 | `Geology` | `Climate` | `Glaciology` | `Hydrology` | `Ecology` | `Domesticates` | `Subsistence` | `Population` | `Settlement` | `Polity` | `Conflict` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 地殻形成期 | 高 | 低 | 低 | なし | なし | なし | なし | なし | なし | なし | なし |
| 環境形成期 | 中 | 高 | 高 | 高 | 低 | なし | なし | なし | なし | なし | なし |
| 先史期 | 低 | 中 | 中 | 中 | 高 | 中 | 中 | 低 | なし | なし | なし |
| 文明成立期 | 低 | 低 | 低 | 中 | 中 | 中 | 高 | 高 | 高 | 高 | 低 |
| 歴史展開期 | 低 | 低 | 低 | 低 | 低 | 低 | 中 | 高 | 高 | 高 | 高 |

`Glaciology` は独立モジュールとして `Climate` と `Hydrology` の間で実行する。
現行実装では `Climate` 予算を流用して更新する。

歴史展開期のように活動量が低いModuleはスキップ可能とする。
スキップ条件の閾値は後続バージョンで定義する。

実際の更新は `System` 単位で行う。
`ExecSystem` は時代・状態・予算を参照して、各Module内で実行する `System` を選択する。

```rust
// 擬似型: Moduleごとの実行対象System列
type SystemPlan = HashMap<ModuleId, Vec<SystemId>>;
```

## 時代遷移

時代遷移は固定tick数で行う。状態条件による遷移判定は持たない。
テストのしやすさを担保するため。 人類が生まれるタイミングとかは管理できないが、
地形から人類の活動が制約されるという目的は十分表現できると判断した。

```rust
struct EpochTransition {
    at_tick: u32,   // このtickの開始時に次のEpochへ遷移する
}

const EPOCH_TRANSITIONS: &[(Epoch, EpochTransition)] = &[
    (Epoch::Geological,  EpochTransition { at_tick:     0 }),  // tick   0: 地殻形成期 開始
    (Epoch::Climate,     EpochTransition { at_tick:   800 }),  // tick 800: 環境形成期 開始（-5億年相当）
    (Epoch::Ecology,     EpochTransition { at_tick: 1_300 }),  // tick 1300: 先史期 開始（-10万年相当）
    (Epoch::Society,     EpochTransition { at_tick: 1_395 }),  // tick 1395: 文明成立期 開始（-5500年相当）
    (Epoch::History,     EpochTransition { at_tick: 1_445 }),  // tick 1445: 歴史展開期 開始（-500年相当）
];
```

`ExecSystem` は各tick終了時に `clock.tick + 1` を参照し、次tick開始時点で有効になるEpochを決める。
遷移は固定tick一致のみで発生し、min_ticks・max_ticks・状態条件によるガードは持たない。

## 更新ループ

1tickの標準順序は次の通り。

1. tick開始時に `ExecSystem` が `FeedbackQueue` の内容を一括で `CellStore` と `EntityStore` に適用する
2. `Geology`
3. `Climate`
4. `Glaciology`
5. `Terrain` 再構成（共有状態層の更新：緯度・海からの距離・海岸線・隣接情報）
6. `Hydrology`
7. `Ecology`
8. `Domesticates`
9. `Subsistence`
10. `Population`
11. `Settlement`
12. `Polity`
13. `Conflict`
14. 各モジュールが次tick向けの影響を `FeedbackQueue` に格納する
15. 時代遷移判定（tick終了時に、次tickの `clock.tick + 1` が `EPOCH_TRANSITIONS` の `at_tick` に一致する場合、Epochを更新する）

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
