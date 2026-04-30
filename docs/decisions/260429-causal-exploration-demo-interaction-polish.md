# 260429 Causal Exploration Demo Interaction Polish

## Status

Superseded

Reason: 因果探索 Demo 実装の撤去に伴い、本 Decision の適用対象が消滅したため

## Context

因果探索 Demo は、世界の中を観察しながら痕跡を辿る体験を意図している。
しかし現行実装では loading overlay が前面 canvas として残り、通常時の pointer 入力を遮るため、globe 回転が不安定または不能になる。
また、feature / trace 選択が `pointerdown` で確定するため、回転開始時に誤選択が起きやすい。

## Decision

- loading overlay canvas は表示専用とし、pointer 入力を受けない
- Demo 内の選択確定は click 相当操作で行い、ドラッグ移動時は選択しない
- Demo overlay には短い操作ヒントを表示し、最初の能動操作後は自動で隠す

## Consequences

- globe 回転と Demo 探索が競合しにくくなる
- 初見ユーザーの入口は改善されるが、長文ガイドなしの観察中心 UI は維持される
- Wasm API や Demo DTO は変更しない
