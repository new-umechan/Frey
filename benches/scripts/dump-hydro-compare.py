#!/usr/bin/env python3
"""
Hydrology ベンチマークの実データとシミュレーション出力を比較するスクリプト。

代表地点（アマゾン河口、コンゴ河口など）の値を比較し、
単位・スケールの不一致を特定する。
"""

from __future__ import annotations

import argparse
import struct
import math
from pathlib import Path
from typing import Dict, List, Tuple


# 代表地点の定義（緯度経度）
REPRESENTATIVE_REGIONS = {
    "amazon_mouth": (-1.5, -51.5),      # アマゾン河口
    "congo_mouth": (-6.0, 12.5),         # コンゴ河口
    "mississippi_mouth": (29.0, -89.5),  # ミシシッピ河口
    "yangtze_mouth": (31.5, 121.5),      # 長江河口
    "nile_mouth": (31.5, 31.0),          # ナイル河口
    "sahara_interior": (23.0, 13.0),     # サハラ内部
    "himalaya_foothills": (27.0, 85.0),  # ヒマラヤ山麓
    "ganges_delta": (22.5, 89.5),        # ガンジスデルタ
}


def haversine(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    """2 点間の距離（km）を計算"""
    R = 6371.0  # 地球半径 km
    lat1_r, lon1_r = math.radians(lat1), math.radians(lon1)
    lat2_r, lon2_r = math.radians(lat2), math.radians(lon2)
    dlat = lat2_r - lat1_r
    dlon = lon2_r - lon1_r
    a = math.sin(dlat / 2) ** 2 + math.cos(lat1_r) * math.cos(lat2_r) * math.sin(dlon / 2) ** 2
    c = 2 * math.asin(math.sqrt(a))
    return R * c


def load_centroids(path: Path) -> List[Tuple[int, float, float]]:
    """セル重心ファイルを読み込む [(cell_id, lat, lon), ...]"""
    cells = []
    with open(path, 'r') as f:
        header = f.readline()  # skip header
        for line in f:
            parts = line.strip().split(',')
            if len(parts) >= 3:
                cell_id = int(parts[0])
                lat = float(parts[1])
                lon = float(parts[2])
                cells.append((cell_id, lat, lon))
    return cells


def find_nearest_cell(cells: List[Tuple[int, float, float]], lat: float, lon: float) -> Tuple[int, float]:
    """指定緯度経度に最も近いセルを返す (cell_id, distance_km)"""
    min_dist = float('inf')
    nearest_id = -1
    for cell_id, cell_lat, cell_lon in cells:
        dist = haversine(lat, lon, cell_lat, cell_lon)
        if dist < min_dist:
            min_dist = dist
            nearest_id = cell_id
    return nearest_id, min_dist


def load_hydro_ref(path: Path) -> Tuple[List[float], List[int]]:
    """hydro_ref.bin を読み込む (river_flow, is_lake)"""
    with open(path, 'rb') as f:
        magic = f.read(9).decode('ascii')
        if magic != 'HYDROREF1':
            raise ValueError(f"Invalid magic: {magic}")
        
        version = struct.unpack('<I', f.read(4))[0]
        cell_count = struct.unpack('<Q', f.read(8))[0]
        
        river_flow = list(struct.unpack(f'<{cell_count}f', f.read(cell_count * 4)))
        is_lake = list(f.read(cell_count))
        
        return river_flow, is_lake


def load_hydro_input(path: Path) -> List[float]:
    """hydro_input.bin を読み込む (runoff)"""
    with open(path, 'rb') as f:
        magic = f.read(9).decode('ascii')
        if magic != 'HYDINPUT1':
            raise ValueError(f"Invalid magic: {magic}")
        
        version = struct.unpack('<I', f.read(4))[0]
        cell_count = struct.unpack('<Q', f.read(8))[0]
        
        runoff = list(struct.unpack(f'<{cell_count}f', f.read(cell_count * 4)))
        
        return runoff


def load_terrain_ref(path: Path) -> List[float]:
    """terrain_ref.bin を読み込む (height)"""
    with open(path, 'rb') as f:
        magic = f.read(8).decode('ascii')
        if magic != 'TERRREF1':
            raise ValueError(f"Invalid magic: {magic}")
        
        version = struct.unpack('<I', f.read(4))[0]
        cell_count = struct.unpack('<Q', f.read(8))[0]
        
        height = list(struct.unpack(f'<{cell_count}f', f.read(cell_count * 4)))
        
        return height


def main():
    parser = argparse.ArgumentParser(description='Hydrology 実データとシミュレーション出力を比較')
    parser.add_argument('--repo-root', default='.', help='リポジトリルート')
    args = parser.parse_args()
    
    repo = Path(args.repo_root).resolve()
    
    # ファイルパス
    centroids_path = repo / 'benches/data/cell_centroids.csv'
    hydro_ref_path = repo / 'benches/data/hydro_ref.bin'
    hydro_input_path = repo / 'benches/data/hydro_input.bin'
    terrain_ref_path = repo / 'benches/data/terrain_ref.bin'
    
    # ファイル存在確認
    for path in [centroids_path, hydro_ref_path, hydro_input_path, terrain_ref_path]:
        if not path.exists():
            print(f"ERROR: File not found: {path}")
            return 1
    
    # データ読み込み
    print("Loading data...")
    cells = load_centroids(centroids_path)
    river_flow_ref, is_lake_ref = load_hydro_ref(hydro_ref_path)
    runoff_input = load_hydro_input(hydro_input_path)
    height = load_terrain_ref(terrain_ref_path)
    
    cell_count = len(cells)
    print(f"Cell count: {cell_count}")
    print()
    
    # 統計情報
    finite_flow = [v for v in river_flow_ref if math.isfinite(v)]
    print(f"Reference river_flow stats:")
    print(f"  Finite cells: {len(finite_flow)}/{cell_count}")
    if finite_flow:
        print(f"  Min: {min(finite_flow):.4f} m³/s")
        print(f"  Max: {max(finite_flow):.4f} m³/s")
        print(f"  Mean: {sum(finite_flow)/len(finite_flow):.4f} m³/s")
    print()
    
    positive_runoff = [v for v in runoff_input if v > 0]
    print(f"Input runoff stats:")
    print(f"  Positive cells: {len(positive_runoff)}/{cell_count}")
    if positive_runoff:
        print(f"  Min: {min(positive_runoff):.2f} mm/yr")
        print(f"  Max: {max(positive_runoff):.2f} mm/yr")
        print(f"  Mean: {sum(positive_runoff)/len(positive_runoff):.2f} mm/yr")
    print()
    
    # 代表地点の値
    print("=" * 70)
    print("REPRESENTATIVE REGIONS COMPARISON")
    print("=" * 70)
    print()
    
    print(f"{'Region':<20} {'Lat':>8} {'Lon':>10} {'Cell':>6} {'Dist':>8} {'RiverFlow':>14} {'Runoff':>12} {'Height':>10}")
    print(f"{'':20} {'deg':>8} {'deg':>10} {'ID':>6} {'km':>8} {'m³/s':>14} {'mm/yr':>12} {'norm':>10}")
    print("-" * 100)
    
    for region_name, (lat, lon) in REPRESENTATIVE_REGIONS.items():
        cell_id, dist_km = find_nearest_cell(cells, lat, lon)
        
        if 0 <= cell_id < cell_count:
            flow_val = river_flow_ref[cell_id]
            runoff_val = runoff_input[cell_id]
            height_val = height[cell_id]
            
            flow_str = f"{flow_val:>14.4f}" if math.isfinite(flow_val) else "N/A"
            runoff_str = f"{runoff_val:>12.2f}" if math.isfinite(runoff_val) else "N/A"
            height_str = f"{height_val:>10.4f}" if math.isfinite(height_val) else "N/A"
            
            print(f"{region_name:<20} {lat:>8.2f} {lon:>10.2f} {cell_id:>6} {dist_km:>8.2f} {flow_str} {runoff_str} {height_str}")
    
    print()
    
    # 考察
    print("=" * 70)
    print("ANALYSIS")
    print("=" * 70)
    print()
    
    # アマゾン河口の値をチェック
    amazon_lat, amazon_lon = REPRESENTATIVE_REGIONS["amazon_mouth"]
    amazon_cell, _ = find_nearest_cell(cells, amazon_lat, amazon_lon)
    amazon_flow = river_flow_ref[amazon_cell]
    amazon_runoff = runoff_input[amazon_cell]
    
    print(f"Amazon mouth (cell {amazon_cell}):")
    print(f"  Reference river_flow: {amazon_flow:.2f} m³/s")
    print(f"  Input runoff: {amazon_runoff:.2f} mm/yr")
    print()
    
    # 期待値との比較
    expected_amazon_flow = 200000  # m³/s (アマゾン川の実流量)
    if amazon_flow > 0 and amazon_flow < expected_amazon_flow * 0.01:
        print(f"WARNING: Amazon flow is {amazon_flow:.2f} m³/s, expected ~{expected_amazon_flow} m³/s")
        print(f"         This is {amazon_flow/expected_amazon_flow*100:.2f}% of expected value.")
        print()
        print("Possible issues:")
        print("  1. Unit conversion error (mm/yr to m³/s)")
        print("  2. Cell area not considered in flow calculation")
        print("  3. Runoff input is too low")
        print("  4. River network not properly built (only 1 tick)")
    
    # セル面積の概算
    # 地球表面積 = 510.1 百万 km²
    # レベル 6 の正二十面体分割: 約 40962 セル
    earth_surface_km2 = 510.1e6
    cell_area_km2 = earth_surface_km2 / cell_count
    print()
    print(f"Estimated cell area: {cell_area_km2:.2f} km²")
    
    # runoff から流量への簡易変換
    # 1 mm/yr = 0.001 m/yr
    # 流量 (m³/s) = runoff (m/yr) × area (m²) / seconds_per_year
    seconds_per_year = 365.25 * 24 * 3600
    if amazon_runoff > 0:
        runoff_m_yr = amazon_runoff / 1000.0  # mm/yr → m/yr
        cell_area_m2 = cell_area_km2 * 1e6  # km² → m²
        estimated_flow = runoff_m_yr * cell_area_m2 / seconds_per_year
        print()
        print(f"Estimated flow from runoff (single cell):")
        print(f"  runoff = {amazon_runoff} mm/yr = {runoff_m_yr:.4f} m/yr")
        print(f"  cell_area = {cell_area_km2:.2f} km² = {cell_area_m2:.2e} m²")
        print(f"  estimated_flow = {estimated_flow:.4f} m³/s")
        print()
        print(f"  Note: This is for ONE cell. River flow accumulates from upstream cells.")


if __name__ == "__main__":
    import sys
    sys.exit(main())
