# 時代とtick制御

## 目的

この文書は、時代、tick、予算、時代遷移、更新順序を定義する。  
ここで扱うのは時間制御と実行制御であり、データ構造の正本は `docs/reference/architecture/data_model.md` を参照する。

## ClockState

tick は単なるカウンタではなく、時代に応じた実時間スケールを持つ。

```rust
struct ClockState {
    tick: u64,
    epoch: EraKind,
    real_years_per_tick: f32,
    runtime_tick_ms: u32,
    budgets: SubsystemBudgets,
    transition: TransitionState,
}
```

- `real_years_per_tick` と `runtime_tick_ms` は `epoch` から毎 tick の `Prepare` で再設定する
- `runtime_tick_ms` は実行速度制御の目安であり、シミュレーション結果の正本ではない

## 時代一覧

| 時代 | 主対象 | 開始年 | 終了年 | 1tick | tick数 | 累計tick | 重点 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 地殻形成期 | `Geology` | -45億年 | -5億年 | 500万年 | 800 | 800 | プレート運動、境界活動、海陸骨格 |
| 環境形成期 | `Climate` / `Hydrology` | -5億年 | -10万年 | 100万年 | 500 | 1,300 | 降水、流出、流路、流量、侵食、堆積 |
| 先史期 | `Ecology` / `Domesticates` / `Subsistence` | -10万年 | -5,500年 | 1,000年 | 95 | 1,395 | 可住性、生産性、作物・家畜分布、生業成立 |
| 文明成立期 | `Population` / `Settlement` / `Polity` | -5,500年 | -500年 | 100年 | 50 | 1,445 | 定住、都市化、初期国家形成 |
| 歴史展開期 | `Conflict`（+Tier 2） | -500年 | — | 1年 | 上限なし | 1,445〜 | 国家競合、戦争、交易、技術変化 |

歴史展開期の終了年は定義しない。何年まで回すかは実行時に決める。

## 予算配分

`SubsystemBudgets` は、各大分類に与える内部更新回数の近似値である。

```rust
struct SubsystemBudgets {
    geology: u32,
    climate: u32,
    ecology: u32,
    civilization: u32,
}
```

| 時代 | `geology` | `climate` | `ecology` | `civilization` |
| --- | --- | --- | --- | --- |
| 地殻形成期 | 高 | 低 | なし | なし |
| 環境形成期 | 中 | 高 | 低 | なし |
| 先史期 | 低 | 中 | 高 | 中 |
| 文明成立期 | 低 | 低 | 中 | 高 |
| 歴史展開期 | 低 | 低 | 低 | 高 |

## 時代遷移

時代遷移は固定 tick 数で行う。状態条件による遷移判定は持たない。

```rust
const EPOCH_TRANSITIONS: &[(EraKind, u64)] = &[
    (EraKind::Crust, 0),
    (EraKind::Environment, 800),
    (EraKind::Life, 1_300),
    (EraKind::Civilization, 1_395),
    (EraKind::History, 1_445),
];
```

`Transition` phase は tick 末尾で `clock.tick + 1` を参照し、次 tick で有効になる `epoch` を決める。

## 実行順序の正本

実行順は hand-written な if/match ではなく `ModuleDeclaration` を正本にする。

```rust
struct ModuleDeclaration {
    phase: ExecWorldPhase,
    module_id: ModuleId,
    reads: &'static [WorldResource],
    writes: &'static [WorldResource],
    feedback: &'static [ModuleId],
    feedback_mode: FeedbackMode,
    profile_category: ProfileCategory,
    display_group: DisplayGroup,
    execution_kind: ExecutionKind,
    completes_tick: bool,
    step: fn(&mut World, &mut ModuleExecContext<'_>),
}
```

依存は declaration から自動生成する。

- `writes -> reads/writes` の資源競合から依存 edge を作る
- `feedback -> target module` から inbox 依存 edge を作る
- 実行順は topo sort で決め、同順位は declaration 定義順で安定化する

## 1tick の標準シーケンス

`declared_phase_order()` に従って次を行う。

1. 現在 phase の declaration を取得する
2. `phase_accepts_module_feedback(phase)` の場合は、対象 module 宛て feedback のみ適用する
3. declaration の `step(world, ctx)` を実行する
4. `phase_completes_tick(phase)` なら tick 完了として `clock.tick` を進める

補足:

- `ExecFeedback` は `ModuleId::Exec` inbox を処理する専用 phase
- module 間の逆向き影響は feedback queue 経由で次 tick へ遅延する
- 同一 tick 内で逆向き即時反映は行わない

## スライス実行

Web/Worker では work budget 付きの slice 実行を使う。

```rust
struct ExecWorldSliceResult {
    next_phase: ExecWorldPhase,
    ticks_completed: u32,
    work_units_consumed: u32,
}
```

`next_phase` は declaration ベースの `next_phase_after()` で決まる。  
`ticks_completed > 0` になった時点で slice を返す。

## 並列化と再現性

WASM では現時点でシングルスレッド実行とする。  
将来並列化する場合も、同一 seed・同一パラメータで再現性を壊さないことを優先し、
順序依存がある計算は逐次実行を維持する。
