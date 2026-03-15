"""
generate_sample.py
Proto #0.1 用サンプル波形データ（Parquet）を生成する。

想定: モータ制御っぽい波形
  - ch1: U相電圧 (50Hz 正弦波, 200V振幅)
  - ch2: U相電流 (50Hz 正弦波, 10A振幅, 30deg遅れ)
"""

import numpy as np
import pandas as pd
from pathlib import Path

# --- パラメータ ---
SAMPLE_RATE = 10_000   # 10kHz
DURATION    = 0.1      # 100ms
FREQ        = 50       # 基本波 50Hz

# --- 波形生成 ---
t = np.linspace(0, DURATION, int(SAMPLE_RATE * DURATION), endpoint=False)
voltage = 200.0 * np.sin(2 * np.pi * FREQ * t)
current = 10.0  * np.sin(2 * np.pi * FREQ * t - np.deg2rad(30))

# --- DataFrame → Parquet ---
df = pd.DataFrame({
    "time":       t,
    "voltage_u":  voltage,
    "current_u":  current,
})

out_path = Path(__file__).parent / "sample_data" / "sample_waveform.parquet"
out_path.parent.mkdir(exist_ok=True)
df.to_parquet(out_path, index=False)

print(f"生成完了: {out_path}")
print(f"  サンプル数: {len(df):,} 点 / サンプリング: {SAMPLE_RATE} Hz / 時間: {DURATION*1000:.0f} ms")
