# 260428 Causal Exploration Demo Slice

## Status

Superseded

Reason: 因果探索 Demo Slice 実装をコードベースから撤去したため

## Note

この decision は、因果探索モードの恒久仕様を固定するためのものではない。
今回の体験検証 experiment をぶらさず実装するための、暫定的な制約だけを記録する。

experiment の結果次第で、この decision は破棄、置換、または proposal への吸収を行ってよい。

## Context

`docs/proposal/causal-exploration-mode.md` では、因果探索モードを全体仕様としてではなく、体験が成立するかを確かめる Demo Slice として扱う。
ただし、実装に入ると対象数、trace 数、relation の意味づけ、UI の演出責務がぶれやすい。

そのため、今回の experiment に限って守る制約を、実装前に明文化する。

## Decision

- 初回は `border_mountain_plate_demo` という静的 Demo Slice を実装する
- `world_id` は存在確認にだけ使い、データ本体は固定値で返す
- 返す対象は `border_segment`、`ridge_or_mountain_band`、`tectonic_compression_or_plate_boundary` の 3 feature に固定する
- trace は `ridge_alignment`、`passability_break`、`tectonic_driver` の 3 本に固定する
- relation type は `constraint_alignment`、`geomorphic_structure`、`tectonic_driver` の 3 種だけを使う
- 国境と山脈の関係は直接因果ではなく `constraint_alignment` として扱う
- UI は DTO に含まれない因果を補完しない
- 色、太さ、流速、揺らぎは `display_mapping` からだけ導く
- evidence には `assumptions`、`approximations`、`uncertainty_reason` を必ず含める
- 詳細な根拠パネルは今回の対象外とし、初回は evidence 種別と不確実性理由の短い表示だけを許可する

## Consequences

- 実装は全モジュール統合前でも着手できる
- この experiment では UI の探索感と情報密度を先に検証できる
- 一方で、世界生成に基づく真の因果探索や全モジュール連鎖の妥当性は、この decision だけでは検証できない
- 後続で自動抽出や一般化を進める際は、この固定条件を前提にし続けない
