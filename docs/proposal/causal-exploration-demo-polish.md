# 因果探索 Demo 操作改善

## Status

Superseded

Reason: 因果探索 Demo 実装の撤去により本提案の前提が無効化されたため

## 背景

現行 Demo は、globe を観察しながら痕跡を辿る体験を目指している一方で、実際には通常の回転操作が阻害される場面がある。
また、初見ユーザーにとって「まずどこを触るか」が少し弱く、探索の入口が立ち上がりにくい。

## 目的

- globe 回転を常時使える状態へ戻す
- Demo の hover / click とカメラ操作を競合させない
- 世界観を壊さない薄い導線だけを加え、初見でも探索を始めやすくする

## 提案概要

- loading overlay は表示専用とし、平常時の pointer 入力を遮断しない
- causal exploration layer の選択確定は `pointerdown` ではなく click 相当の操作で行い、ドラッグ回転中の誤選択を減らす
- Demo overlay には短い操作ヒントを加え、最初の操作後は自動で退く

## スコープ

含むもの:

- loading overlay と canvas 入力の整理
- causal exploration Demo の軽い操作改善
- UI reference の更新

含まないもの:

- Wasm API の変更
- Demo Slice の feature / trace 構成変更
- 長文チュートリアルや恒久ガイド UI

## 成功条件

- 初期化後は globe をドラッグ回転できる
- trace / feature 選択と回転が同時に成立し、誤選択が減る
- 初見でも最低限の操作意図が読み取れる

## リスクとトレードオフ

- click 判定を厳しくしすぎると、短いタップの取りこぼしが起こりうる
- ヒントを増やしすぎると Demo Slice の観察感を損なう

このため、移動量しきい値は小さく保ち、ヒントは短文かつ一時表示に留める。

## 実施計画

1. loading overlay の表示責務と入力責務を分離する
2. causal exploration layer の click / drag 判定を導入する
3. 短い操作ヒントを追加し、最初の能動操作で退かせる
4. テストと UI reference を更新する

## 未解決事項

- 将来の mobile 向け因果探索操作をどこまで touch 専用に最適化するか
- 根拠ビュー追加時に、今回の薄いヒントとどう共存させるか
