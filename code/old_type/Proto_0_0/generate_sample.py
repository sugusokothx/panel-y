import json
import math
import random

SAMPLES = 1000
T_START = 0.0
T_END = 0.04
FREQ = 50  # Hz

time = [T_START + i * (T_END - T_START) / (SAMPLES - 1) for i in range(SAMPLES)]
voltage = [200 * math.sin(2 * math.pi * FREQ * t) for t in time]
current = [10 * math.sin(2 * math.pi * FREQ * t) + random.uniform(-0.5, 0.5) for t in time]

data = {"time": time, "voltage": voltage, "current": current}

with open("public/waveform.json", "w") as f:
    json.dump(data, f)

print(f"Generated {SAMPLES} samples -> public/waveform.json")
