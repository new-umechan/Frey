# World StateとExec State

## 目的

この文書は、何を `World State` に置き、何を `Exec` 側の管理状態に置くかを定義する。

設計上の原則は単純である。
各モジュールが共有して読む現在値は `World State` に置く。
tick進行や履歴管理のための状態は `Exec State` に置く。

## 2つの状態

### `World State`

`World State` は、各セルが持つ属性の現在値である。
モジュールはこれを読んで書く。

例:

- 標高
- プレートID
- 降水
- 流量
- 流路
- 植生
- 可住性
- 人口
- 国家ID
- 技術水準
- 汚染量

重要なのは、これは「現在の世界の面」であって、更新器固有の内部メモリではないことだ。

### `Exec State`

`Exec State` は、世界を進めるための進行管理状態である。
各モジュールの対象世界そのものではない。

例:

- 現在tick
- 現在の時代
- `Tick.real_years`
- `SubsystemBudgets`
- 更新順序
- `FeedbackQueue`
- 履歴
- スナップショット
- 再生速度

## 目標構造

目標の構造は概念上、次のように分ける。

```python
World = {
    state: WorldState,
    exec: ExecState,
}
```

### `WorldState` の例

```python
WorldState = {
    geo: {
        latitude_deg,
        distance_from_ocean_km,
        coast_side,
        is_coastal,
    },
    geology: {
        height,
        plate_id,
        erosion_rate,
        deposition_rate,
        river_path,
    },
    climate: {
        precipitation,
        runoff,
        temperature,
        evapotranspiration,
        aridity,
        ocean_temperature,
    },
    ecology: {
        vegetation,
        habitability,
        productivity,
    },
    civilization: {
        population,
        state_id,
        agriculture,
        pollution,
    },
}
```

### `ExecState` の例

```python
ExecState = {
    tick,
    epoch,
    budgets,
    feedback_queue,
    history,
    snapshots,
}
```

## 更新器との関係

更新器はステートレスに保つ。
つまり、更新器自身は長寿命の内部状態を持たず、共有状態としての `World State` と進行管理入力としての `Exec State` を引数として受け取り、次の状態を書き戻すだけにする。

```python
def update_geology(world_state, exec_state): ...
def update_climate(world_state, exec_state): ...
def update_ecology(world_state, exec_state): ...
def update_civilization(world_state, exec_state): ...
```

`FeedbackQueue` は `Exec State` に置く。
tick N で `Civilization` がここに環境影響を書き込み、tick N+1 の開始時に `Exec` が `World State` へ適用する。

## 現行実装との差分

現行実装は、まだこの目標形に完全には一致していない。

Rust側では `World` に次のような実装都合の保持が残っている。

- `terrain_dynamics`
- `river_erosion_state`

また、WASM管理層やJS側にも次のような管理状態がある。

- 履歴
- スナップショット
- 再生速度
- ランタイム制御状態

これらは目標アーキテクチャでは `Exec State` に寄せて整理する対象である。

## 補足

- 現在の実装型として `core` `layers` などが存在していても、architectureではそれを主語にしない
- architectureで重要なのは、どの値が共有の `World State` で、どの値が進行管理の `Exec State` かである
- 固定地理量のようにtickごとには変化しない値でも、各モジュールが共有して読むなら `World State` に置く
