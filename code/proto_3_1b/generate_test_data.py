"""データ間引きテスト用の大規模波形データを生成する。

100kHz サンプリング × 1秒 = 100,000点
チャンネル:
  - sine_50Hz: 50Hz正弦波（アナログ信号の代表）
  - pwm_1kHz: 1kHz PWM波（デジタル信号、step表示向き）
  - noise: ホワイトノイズ（ランダム、ピーク保持のテスト）
  - chirp: チャープ信号（1Hz→500Hz掃引、ズーム時の解像度テスト）
"""

import numpy as np
import pandas as pd
from pathlib import Path
from scipy import signal

N = 100_000
FS = 100_000  # 100kHz
T = N / FS    # 1秒

t = np.linspace(0, T, N, endpoint=False)

# 50Hz 正弦波 (振幅 ±10)
sine_50 = 10 * np.sin(2 * np.pi * 50 * t)

# 1kHz PWM (duty 30%)
pwm_1k = (signal.square(2 * np.pi * 1000 * t, duty=0.3) + 1) / 2 * 5  # 0-5V

# ホワイトノイズ (±1)
noise = np.random.default_rng(42).normal(0, 1, N)

# チャープ信号 (1Hz→500Hz, 振幅 ±5)
chirp = 5 * signal.chirp(t, f0=1, f1=500, t1=T, method="linear")

df = pd.DataFrame({
    "time": t,
    "sine_50Hz": sine_50,
    "pwm_1kHz": pwm_1k,
    "noise": noise,
    "chirp_1_500Hz": chirp,
})

out = Path(__file__).parent / "data" / "test_100k.parquet"
df.to_parquet(out, index=False)
print(f"生成完了: {out} ({len(df):,}点, {len(df.columns)-1}ch)")
