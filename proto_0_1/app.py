"""
app.py - Panel_y Proto #0.1 "Hello Dash"

起動:
  python app.py
  → http://localhost:8050
"""

from pathlib import Path
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots
import dash
from dash import dcc, html

# --- データ読み込み ---
DATA_PATH = Path(__file__).parent / "sample_data" / "sample_waveform.parquet"

df = pd.read_parquet(DATA_PATH)

# チャンネル列（time以外）を自動取得
channels = [col for col in df.columns if col != "time"]

# --- グラフ作成 ---
fig = make_subplots(
    rows=len(channels),
    cols=1,
    shared_xaxes=True,       # 時間軸同期（ズーム・パン連動）
    vertical_spacing=0.06,
)

for i, ch in enumerate(channels, start=1):
    fig.add_trace(
        go.Scatter(
            x=df["time"],
            y=df[ch],
            mode="lines",
            name=ch,
            line=dict(width=1),
        ),
        row=i,
        col=1,
    )
    fig.update_yaxes(title_text=ch, row=i, col=1)

fig.update_xaxes(
    title_text="Time [s]",
    showspikes=True,
    spikemode="across",
    spikesnap="cursor",
    row=len(channels),
    col=1,
)
fig.update_layout(
    hovermode="x unified",
    height=300 * len(channels),
    margin=dict(t=40, b=40, l=80, r=20),
    title="Panel_y Proto #0.1",
    template="plotly_dark",
)

# --- Dash アプリ ---
app = dash.Dash(__name__)

app.layout = html.Div([
    html.H2("Panel_y — Proto #0.1", style={"fontFamily": "sans-serif", "padding": "12px"}),
    dcc.Graph(
        id="waveform",
        figure=fig,
        config={"scrollZoom": True},   # マウスホイールでズーム可
        style={"height": f"{300 * len(channels)}px"},
    ),
], style={"backgroundColor": "#1a1a1a", "minHeight": "100vh"})

if __name__ == "__main__":
    print(f"データ: {DATA_PATH}")
    print(f"チャンネル: {channels}")
    app.run(debug=True)
