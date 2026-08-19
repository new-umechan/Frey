# Persistent materialの数値的dust閾値を拡大する

## Status

Rejected

## Context

level 6・alphaの事前計算はupdate 1244で、面積 `3.2626456e-8` のpersistent material
elementをfixed meshへ投影できず停止した。このfragmentは平均cell面積の0.010635%であり、
現行のdust除去閾値0.0100%をわずかに上回る。

このサイズでは`f32` gnomonic polygon projectionの候補cell探索が安定せず、地質学的に意味のある
plate境界形状より数値的な細片の保持が計算継続を妨げている。

## Proposal

平均cell面積に対するnumerical dust閾値を0.0100%から0.0125%へ上げる案を評価した。閾値未満の
elementを既存どおり反応前後に破棄することで停止は回避できるが、投影失敗の原因を解決せず、
material履歴を追加で捨てるだけになる。

## Scientific basis

persistent materialは球面上のLagrangian surfaceを固定meshへ投影する数値表現であり、極小fragmentの
除去は地質モデルではなく有限精度のpolygon投影を安定化するためのcutoffである。面積収支と
gap/overlap診断は引き続き監視する。

## Trade-off

平均cell面積の0.0125%未満の局所的なmaterial履歴を失う。投影clipの絶対epsilonが極細triangleの
交差を落としている可能性があり、原因を修正せずにfragmentを捨てるため採用しない。

## Close when

投影失敗はdust閾値ではなくclip計算のスケール依存性を診断して修正する。
