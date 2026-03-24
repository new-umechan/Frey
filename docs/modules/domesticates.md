# Domesticatesの詳細設計

## 入力

## 出力

```rust
// u8で7種の作物をビット管理
// bit0: Wheat, bit1: Rice, bit2: Maize, bit3: Millet
// bit4: Tuber, bit5: Legume, bit6: Barley
type CropBitmap = u8;

// u8で5種の家畜をビット管理
// bit0: Cattle, bit1: Horse, bit2: Sheep, bit3: Pig, bit4: Camel
type LivestockBitmap = u8;
```

