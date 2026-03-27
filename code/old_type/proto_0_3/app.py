"""
app.py - Panel_y Proto #0.3 "Multi-Channel Overlay"

Proto #0.1 ベースに、同一行への複数チャンネル重ね表示を追加。
テキストエリアで行グルーピングを定義する（案E）。

記法:
  - 1行 = 1波形行
  - カンマ区切りでチャンネルをグルーピング
  - 例: id_ref, id

起動:
  python app.py → http://localhost:8050
"""

from pathlib import Path
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots
import dash
from dash import dcc, html, Input, Output, State

# --- データ読み込み ---
DATA_PATH = Path(__file__).parent / "sample_data" / "sample_waveform.parquet"
df = pd.read_parquet(DATA_PATH)
channels = [col for col in df.columns if col != "time"]

# --- 色パレット ---
TRACE_COLORS = [
    "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728",
    "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
]

# --- デフォルトのレイアウト定義 ---
DEFAULT_LAYOUT = "id_ref, id\niq_ref, iq\nvoltage_u\nvoltage_v"


def parse_layout(text: str) -> list[list[str]]:
    """テキストから行グルーピングをパースする。"""
    rows = []
    for line in text.strip().splitlines():
        line = line.strip()
        if not line:
            continue
        chs = [ch.strip() for ch in line.split(",") if ch.strip()]
        # データに存在するチャンネルのみ残す
        valid = [ch for ch in chs if ch in df.columns and ch != "time"]
        if valid:
            rows.append(valid)
    return rows


def build_figure(row_groups: list[list[str]]) -> go.Figure:
    """行グルーピングに従ってFigureを生成する。"""
    n_rows = len(row_groups)
    if n_rows == 0:
        return go.Figure()

    fig = make_subplots(
        rows=n_rows,
        cols=1,
        shared_xaxes=True,
        vertical_spacing=0.06,
    )

    for i, group in enumerate(row_groups, start=1):
        for j, ch in enumerate(group):
            fig.add_trace(
                go.Scatter(
                    x=df["time"],
                    y=df[ch],
                    mode="lines",
                    name=ch,
                    line=dict(width=1, color=TRACE_COLORS[j % len(TRACE_COLORS)]),
                ),
                row=i,
                col=1,
            )
        fig.update_yaxes(title_text=" / ".join(group), row=i, col=1)

    fig.update_xaxes(
        title_text="Time [s]",
        showspikes=True,
        spikemode="across",
        spikesnap="cursor",
        row=n_rows,
        col=1,
    )
    fig.update_layout(
        hovermode="x unified",
        height=300 * n_rows,
        margin=dict(t=40, b=40, l=80, r=20),
        title="Panel_y Proto #0.3",
        template="plotly_dark",
    )
    return fig


# --- Dash アプリ ---
app = dash.Dash(__name__)

app.layout = html.Div([
    html.H2("Panel_y — Proto #0.3", style={
        "fontFamily": "sans-serif", "padding": "12px", "color": "white",
    }),

    # レイアウト定義エリア
    html.Div([
        html.Div([
            html.Label("レイアウト定義", style={
                "color": "#aaa", "fontSize": "12px", "marginBottom": "4px",
            }),
            html.Div(
                f"1行=1波形行、カンマ区切りで重ね表示",
                style={"color": "#666", "fontSize": "11px", "marginBottom": "8px"},
            ),
            dcc.Textarea(
                id="layout-input",
                value=DEFAULT_LAYOUT,
                style={
                    "width": "100%", "height": "120px",
                    "backgroundColor": "#2a2a2a", "color": "#e0e0e0",
                    "border": "1px solid #444", "borderRadius": "4px",
                    "fontFamily": "monospace", "fontSize": "14px",
                    "padding": "8px", "resize": "vertical",
                },
            ),
            html.Button("更新", id="update-btn", n_clicks=0, style={
                "marginTop": "8px", "padding": "6px 20px",
                "backgroundColor": "#444", "color": "white",
                "border": "1px solid #666", "borderRadius": "4px",
                "cursor": "pointer",
            }),
        ], style={"flex": "1"}),

        html.Div([
            html.Label("利用可能チャンネル", style={
                "color": "#aaa", "fontSize": "12px", "marginBottom": "4px",
            }),
            html.Div(
                ", ".join(channels),
                style={
                    "color": "#7ec8e3", "fontFamily": "monospace",
                    "fontSize": "13px", "lineHeight": "1.8",
                },
            ),
        ], style={"marginLeft": "24px", "minWidth": "200px"}),
    ], style={
        "display": "flex", "padding": "0 16px 16px",
        "alignItems": "flex-start",
    }),

    # 波形表示エリア
    html.Div(id="graph-container"),

], style={"backgroundColor": "#1a1a1a", "minHeight": "100vh"})


@app.callback(
    Output("graph-container", "children"),
    Input("update-btn", "n_clicks"),
    State("layout-input", "value"),
)
def update_graph(n_clicks, layout_text):
    row_groups = parse_layout(layout_text)
    if not row_groups:
        return html.Div("有効なチャンネルがありません", style={"color": "#888", "padding": "20px"})

    fig = build_figure(row_groups)
    n_rows = len(row_groups)

    return dcc.Graph(
        id="waveform",
        figure=fig,
        config={"scrollZoom": True},
        style={"height": f"{300 * n_rows}px"},
    )


if __name__ == "__main__":
    print(f"データ: {DATA_PATH}")
    print(f"チャンネル: {channels}")
    app.run(debug=True)
