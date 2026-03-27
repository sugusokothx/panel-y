"""
generate_sample.py
Proto #0.3 用サンプル波形データ（Parquet）を生成する。

想定: モータ制御ステップ応答
  - id_ref: d軸電流指令（ステップ入力）
  - id:     d軸電流（一次遅れ応答）
  - iq_ref: q軸電流指令（ステップ入力）
  - iq:     q軸電流（一次遅れ応答）
  - voltage_u: U相電圧
  - voltage_v: V相電圧
"""

import numpy as np
import pandas as pd
from pathlib import Path

# --- パラメータ ---
SAMPLE_RATE = 10_000   # 10kHz
DURATION    = 0.1      # 100ms
FREQ        = 50       # 基本波 50Hz
TAU         = 0.005    # 電流応答の時定数 5ms

t = np.linspace(0, DURATION, int(SAMPLE_RATE * DURATION), endpoint=False)
dt = 1 / SAMPLE_RATE

# --- ステップ応答生成 ---
def step_response(t, step_time, amplitude, tau):
    """一次遅れステップ応答"""
    ref = np.where(t >= step_time, amplitude, 0.0)
    actual = np.zeros_like(t)
    for i in range(1, len(t)):
        alpha = dt / (tau + dt)
        actual[i] = actual[i - 1] + alpha * (ref[i] - actual[i - 1])
    # ノイズ追加
    actual += np.random.normal(0, amplitude * 0.02, len(t))
    return ref, actual

# d軸電流: 0.02s で 5A ステップ
id_ref, id_act = step_response(t, step_time=0.02, amplitude=5.0, tau=TAU)

# q軸電流: 0.01s で 10A ステップ
iq_ref, iq_act = step_response(t, step_time=0.01, amplitude=10.0, tau=TAU)

# 電圧波形
voltage_u = 200.0 * np.sin(2 * np.pi * FREQ * t)
voltage_v = 200.0 * np.sin(2 * np.pi * FREQ * t - np.deg2rad(120))

# --- DataFrame → Parquet ---
df = pd.DataFrame({
    "time":      t,
    "id_ref":    id_ref,
    "id":        id_act,
    "iq_ref":    iq_ref,
    "iq":        iq_act,
    "voltage_u": voltage_u,
    "voltage_v": voltage_v,
})

out_path = Path(__file__).parent / "sample_data" / "sample_waveform.parquet"
out_path.parent.mkdir(exist_ok=True)
df.to_parquet(out_path, index=False)

print(f"生成完了: {out_path}")
print(f"  サンプル数: {len(df):,} 点 / サンプリング: {SAMPLE_RATE} Hz / 時間: {DURATION*1000:.0f} ms")
print(f"  チャンネル: {[c for c in df.columns if c != 'time']}")
