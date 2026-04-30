"""Generate a small Parquet dataset for Rust step-rendering validation.

The shifted PWM columns make one-sample edge offsets visible when the channels
are overlaid in Step mode.
"""

from pathlib import Path

import numpy as np
import pandas as pd


N = 100_000
FS = 100_000.0
PWM_FREQ = 1_000.0
DUTY = 0.3


def shift_delay_one_sample(values: np.ndarray) -> np.ndarray:
    shifted = np.empty_like(values)
    shifted[0] = values[0]
    shifted[1:] = values[:-1]
    return shifted


def shift_advance_one_sample(values: np.ndarray) -> np.ndarray:
    shifted = np.empty_like(values)
    shifted[:-1] = values[1:]
    shifted[-1] = values[-1]
    return shifted


def main() -> None:
    t = np.arange(N, dtype=np.float64) / FS
    pwm = ((np.mod(t * PWM_FREQ, 1.0) < DUTY).astype(np.float32)) * 5.0

    df = pd.DataFrame(
        {
            "time": t,
            "pwm_1kHz": pwm,
            "pwm_1kHz_delay_1sample": shift_delay_one_sample(pwm),
            "pwm_1kHz_advance_1sample": shift_advance_one_sample(pwm),
        }
    )

    out = Path(__file__).parent / "data" / "step_validation_100k.parquet"
    out.parent.mkdir(exist_ok=True)
    df.to_parquet(out, index=False)
    print(f"generated: {out} ({len(df):,} rows, {len(df.columns) - 1} channels)")


if __name__ == "__main__":
    main()
