# 侵食

侵食は従来の同期一括処理ではなく、地形内部Tickに応じて増分適用する。

- 毎内部Tickに少量実行する
- または `budget_ticks` 内で一定回数だけ実行する
- 変化量上限を守る
- 河川再計算頻度を地形更新頻度と独立に調整してよい

河川は地形更新の影響を受けるため、定期的に再計算する。

本仕様では、実装時の迷いを避けるため、初版の判定ルールを固定する。

河川再計算判定タイミング:
- 各地形内部Tickの終端で判定する

使用する指標（直近内部Tickまたは `step_tectonic_terrain` 内の集計値）:
- `terrain_activity`: 正規化された標高総変化量
- `boundary_activity`: 正規化された境界活動量

合成指標:
- `river_driver = max(terrain_activity, boundary_activity)`

再計算間隔ルール（初版）:
- `river_driver >= river_activity_high_threshold` の間は、`river_rebuild_interval_min` ごとに再計算
- `river_driver <= river_activity_low_threshold` の間は、`river_rebuild_interval_max` ごとに再計算
- その中間は線形補間した間隔を使う

強制再計算条件:
- 海陸反転セル数が閾値を超えたとき
- `step_tectonic_terrain` 呼び出しの最終内部Tick

初期既定値（暫定）:
- `river_rebuild_interval_min = 1`
- `river_rebuild_interval_max = 8`
- `river_activity_high_threshold = 0.03`
- `river_activity_low_threshold = 0.005`

