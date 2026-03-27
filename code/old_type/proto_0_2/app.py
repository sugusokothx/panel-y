"""
app.py - Panel_y Proto #0.2

レイアウト:
  | Channel   | Value  | Waveform Graph |
  |-----------|--------|----------------|
  | voltage_u | 141.4V | ~~~wave~~~     |
  | current_u |   4.3A | ~~~wave~~~     |

修正内容（#0.1からの変更）:
  - [FIX] ホバー値を全チャンネル同時表示
      hoverData → DataFrame逆引き → 各行の値ラベルを更新
  - [FIX] スパイクラインを全パネル縦断表示
      yref="paper" のシェイプを全グラフに動的追加
  - [NEW] 行レイアウト（channel / value / graph）
  - [NEW] グラフ間のズーム・パン同期（relayoutData → 全グラフに反映）

起動:
  python app.py → http://localhost:8050
"""

from pathlib import Path
import pandas as pd
import plotly.graph_objects as go
import dash
from dash import dcc, html, Input, Output, Patch, ctx, no_update

# ---------------------------------------------------------------------------
# データ
# ---------------------------------------------------------------------------
DATA_PATH = Path(__file__).parent.parent / "proto_0_1" / "sample_data" / "sample_waveform.parquet"
df = pd.read_parquet(DATA_PATH)
channels = [col for col in df.columns if col != "time"]
n = len(channels)

GRAPH_HEIGHT = 180  # px / チャンネル行

# ---------------------------------------------------------------------------
# チャンネルごとの Figure
# ---------------------------------------------------------------------------
def make_channel_fig(ch: str, show_xaxis: bool = False) -> go.Figure:
    fig = go.Figure()
    fig.add_trace(go.Scatter(
        x=df["time"],
        y=df[ch],
        mode="lines",
        name=ch,
        line=dict(width=1),
        hoverinfo="none",       # ツールチップ非表示・hoverDataイベントは発火させる
    ))
    fig.update_layout(
        height=GRAPH_HEIGHT,
        margin=dict(t=8, b=30 if show_xaxis else 8, l=10, r=10),
        template="plotly_dark",
        showlegend=False,
        hovermode="x",
        xaxis=dict(
            showticklabels=show_xaxis,
            title="Time [s]" if show_xaxis else "",
        ),
        yaxis=dict(title=ch),
        paper_bgcolor="rgba(0,0,0,0)",
        plot_bgcolor="#1e1e1e",
    )
    return fig

# ---------------------------------------------------------------------------
# スタイル定数
# ---------------------------------------------------------------------------
DARK_BG    = "#1a1a1a"
ROW_BORDER = "1px solid #333"

LABEL_COL_STYLE = {
    "width":       "140px",
    "minWidth":    "140px",
    "padding":     "8px 14px",
    "display":     "flex",
    "flexDirection": "column",
    "justifyContent": "center",
    "borderRight": ROW_BORDER,
    "fontFamily":  "monospace",
}

# ---------------------------------------------------------------------------
# 1行分のレイアウト
# ---------------------------------------------------------------------------
def channel_row(ch: str, is_last: bool) -> html.Div:
    return html.Div([
        # 左列: チャンネル名 + ホバー値
        html.Div([
            html.Span(ch, style={
                "color":      "#aaa",
                "fontSize":   "12px",
                "fontWeight": "bold",
            }),
            html.Span("---", id=f"val-{ch}", style={
                "color":      "#7ec8e3",
                "fontSize":   "20px",
                "marginTop":  "6px",
            }),
        ], style=LABEL_COL_STYLE),

        # 右列: 波形グラフ
        dcc.Graph(
            id=f"graph-{ch}",
            figure=make_channel_fig(ch, show_xaxis=is_last),
            config={"scrollZoom": True, "displayModeBar": "hover", "modeBarButtonsToRemove": ["select2d", "lasso2d"]},
            style={"flex": "1", "height": f"{GRAPH_HEIGHT}px"},
        ),
    ], style={
        "display":         "flex",
        "alignItems":      "stretch",
        "borderBottom":    ROW_BORDER,
        "backgroundColor": DARK_BG,
    })

# ---------------------------------------------------------------------------
# Dash アプリ
# ---------------------------------------------------------------------------
app = dash.Dash(__name__)

app.layout = html.Div([
    html.H2("Panel_y — Proto #0.2", style={
        "color": "white", "fontFamily": "sans-serif",
        "padding": "12px 16px", "margin": 0,
    }),

    # 共有ステート
    dcc.Store(id="hover-x-store"),
    dcc.Store(id="xaxis-range-store"),

    # ヘッダー行
    html.Div([
        html.Div("Channel / Value", style={
            "width": "140px", "padding": "4px 14px",
            "borderRight": ROW_BORDER, "color": "#666",
            "fontFamily": "monospace", "fontSize": "11px",
        }),
        html.Div("Waveform", style={
            "flex": "1", "padding": "4px 12px",
            "color": "#666", "fontFamily": "monospace", "fontSize": "11px",
        }),
    ], style={"display": "flex", "backgroundColor": "#222", "borderBottom": ROW_BORDER}),

    # チャンネル行
    *[channel_row(ch, i == n - 1) for i, ch in enumerate(channels)],

], style={"backgroundColor": DARK_BG, "minHeight": "100vh"})

# ---------------------------------------------------------------------------
# Callback 1: いずれかのグラフのホバー位置を Store に保存
# ---------------------------------------------------------------------------
@app.callback(
    Output("hover-x-store", "data"),
    [Input(f"graph-{ch}", "hoverData") for ch in channels],
)
def store_hover_x(*hover_datas):
    for hd in hover_datas:
        if hd and hd.get("points"):
            return hd["points"][0]["x"]
    return no_update

# ---------------------------------------------------------------------------
# Callback 2: ホバー X 位置から全チャンネルの値を DataFrame 逆引きして表示
# ---------------------------------------------------------------------------
@app.callback(
    [Output(f"val-{ch}", "children") for ch in channels],
    Input("hover-x-store", "data"),
)
def update_values(x_val):
    if x_val is None:
        return ["---"] * n
    idx = (df["time"] - x_val).abs().idxmin()
    return [f"{df[ch].iloc[idx]:.4f}" for ch in channels]

# ---------------------------------------------------------------------------
# Callback 3: ズーム・パン操作の X 軸レンジを Store に保存
# ---------------------------------------------------------------------------
@app.callback(
    Output("xaxis-range-store", "data"),
    [Input(f"graph-{ch}", "relayoutData") for ch in channels],
    prevent_initial_call=True,
)
def store_xaxis_range(*relayout_datas):
    for rd in relayout_datas:
        if rd is None:
            continue
        if "xaxis.range[0]" in rd:
            return [rd["xaxis.range[0]"], rd["xaxis.range[1]"]]
        if "xaxis.autorange" in rd:
            return None     # ダブルクリックで全体に戻す
    return no_update

# ---------------------------------------------------------------------------
# Callback 4: 全グラフにスパイクライン + X 軸レンジ同期を反映
# ---------------------------------------------------------------------------
@app.callback(
    [Output(f"graph-{ch}", "figure") for ch in channels],
    Input("hover-x-store", "data"),
    Input("xaxis-range-store", "data"),
    prevent_initial_call=True,
)
def update_graphs(x_val, xaxis_range):
    triggered = ctx.triggered_id
    results = []

    for ch in channels:
        p = Patch()

        # スパイクライン（全グラフ縦断）
        if triggered == "hover-x-store":
            if x_val is not None:
                p["layout"]["shapes"] = [{
                    "type": "line",
                    "x0": x_val, "x1": x_val,
                    "y0": 0,     "y1": 1,
                    "xref": "x", "yref": "paper",
                    "line": {"color": "rgba(180,180,180,0.7)", "width": 1},
                }]
            else:
                p["layout"]["shapes"] = []

        # X 軸レンジ同期
        if triggered == "xaxis-range-store":
            if xaxis_range:
                p["layout"]["xaxis"]["range"]     = xaxis_range
                p["layout"]["xaxis"]["autorange"] = False
            else:
                p["layout"]["xaxis"]["autorange"] = True

        results.append(p)

    return results


if __name__ == "__main__":
    print(f"データ: {DATA_PATH}")
    print(f"チャンネル: {channels}")
    app.run(debug=True)
