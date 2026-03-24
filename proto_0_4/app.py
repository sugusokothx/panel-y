"""
app.py - Panel_y Proto #0.4 "Multi-Channel Overlay with Dropdown UI"

Proto #0.3 の重ね表示機能を、ドロップダウンUIで操作する方式に変更。
行ごとにマルチセレクトドロップダウンでチャンネルを選択。
行の追加・削除ボタンで動的に行数を変更。

起動:
  python app.py → http://localhost:8050
"""

from pathlib import Path
import pandas as pd
import plotly.graph_objects as go
from plotly.subplots import make_subplots
import dash
from dash import dcc, html, Input, Output, State, ALL, ctx, no_update, Patch

# --- データ読み込み ---
DATA_PATH = Path(__file__).parent / "sample_data" / "sample_waveform.parquet"
df = pd.read_parquet(DATA_PATH)
channels = [col for col in df.columns if col != "time"]

# --- 色パレット ---
TRACE_COLORS = [
    "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728",
    "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
]

# --- デフォルト行グルーピング ---
DEFAULT_ROWS = [
    ["id_ref", "id"],
    ["iq_ref", "iq"],
    ["voltage_u"],
    ["voltage_v"],
]

# --- スタイル定数 ---
DARK_BG = "#1a1a1a"
ROW_BG = "#2a2a2a"
BORDER_COLOR = "#444"
BTN_STYLE = {
    "padding": "6px 16px",
    "backgroundColor": "#444",
    "color": "white",
    "border": f"1px solid #666",
    "borderRadius": "4px",
    "cursor": "pointer",
    "fontSize": "13px",
}
DROPDOWN_STYLE = {
    "backgroundColor": "#2a2a2a",
    "color": "#e0e0e0",
}


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
            if ch in df.columns:
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
        label = " / ".join(group) if group else ""
        fig.update_yaxes(title_text=label, row=i, col=1)

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
        title="Panel_y Proto #0.4",
        template="plotly_dark",
    )
    return fig


def make_row_div(row_index: int, selected: list[str] | None = None) -> html.Div:
    """1行分のドロップダウン行UIを生成する。"""
    return html.Div(
        [
            html.Span(
                f"行{row_index + 1}",
                style={
                    "color": "#aaa",
                    "fontSize": "12px",
                    "minWidth": "32px",
                    "marginRight": "8px",
                    "alignSelf": "center",
                },
            ),
            dcc.Dropdown(
                id={"type": "row-dropdown", "index": row_index},
                options=[{"label": ch, "value": ch} for ch in channels],
                value=selected or [],
                multi=True,
                placeholder="チャンネルを選択...",
                style={"flex": "1", "minWidth": "200px"},
            ),
            html.Button(
                "×",
                id={"type": "row-delete", "index": row_index},
                n_clicks=0,
                style={
                    **BTN_STYLE,
                    "marginLeft": "8px",
                    "padding": "6px 10px",
                    "backgroundColor": "#633",
                },
            ),
        ],
        style={
            "display": "flex",
            "alignItems": "center",
            "padding": "6px 0",
        },
    )


# --- Dash アプリ ---
app = dash.Dash(__name__)

initial_row_divs = [make_row_div(i, row) for i, row in enumerate(DEFAULT_ROWS)]
initial_fig = build_figure(DEFAULT_ROWS)

app.layout = html.Div(
    [
        # ストア: 現在の行数管理
        dcc.Store(id="row-count", data=len(DEFAULT_ROWS)),
        html.H2(
            "Panel_y — Proto #0.4",
            style={
                "fontFamily": "sans-serif",
                "padding": "12px 16px 0",
                "color": "white",
                "margin": "0",
            },
        ),
        html.Div(
            "ドロップダウンで行ごとにチャンネルを選択 / 重ね表示",
            style={
                "color": "#666",
                "fontSize": "12px",
                "padding": "4px 16px 12px",
            },
        ),
        # 行定義エリア
        html.Div(
            [
                html.Div(
                    [
                        html.Button(
                            "+ 行追加",
                            id="add-row-btn",
                            n_clicks=0,
                            style=BTN_STYLE,
                        ),
                        html.Button(
                            "更新",
                            id="update-btn",
                            n_clicks=0,
                            style={
                                **BTN_STYLE,
                                "marginLeft": "8px",
                                "backgroundColor": "#2a6496",
                            },
                        ),
                    ],
                    style={
                        "display": "flex",
                        "marginBottom": "12px",
                    },
                ),
                html.Div(
                    id="rows-container",
                    children=initial_row_divs,
                ),
            ],
            style={
                "padding": "0 16px 16px",
                "maxWidth": "600px",
            },
        ),
        # 波形表示エリア
        html.Div(
            dcc.Graph(
                id="waveform",
                figure=initial_fig,
                config={"scrollZoom": True},
                style={"height": f"{300 * len(DEFAULT_ROWS)}px"},
            ),
            id="graph-container",
        ),
    ],
    style={"backgroundColor": DARK_BG, "minHeight": "100vh"},
)


@app.callback(
    Output("rows-container", "children"),
    Output("row-count", "data"),
    Input("add-row-btn", "n_clicks"),
    Input({"type": "row-delete", "index": ALL}, "n_clicks"),
    State("rows-container", "children"),
    State("row-count", "data"),
    prevent_initial_call=True,
)
def manage_rows(add_clicks, delete_clicks, current_children, row_count):
    """行の追加・削除を管理する。"""
    triggered = ctx.triggered_id

    if triggered == "add-row-btn":
        new_row = make_row_div(row_count)
        current_children.append(new_row)
        return current_children, row_count + 1

    # 削除ボタンが押された場合
    if isinstance(triggered, dict) and triggered.get("type") == "row-delete":
        delete_index = triggered["index"]
        updated = []
        for child in current_children:
            # childのpropsからドロップダウンのindexを取得
            row_div_children = child["props"]["children"]
            dropdown = row_div_children[1]  # 2番目がドロップダウン
            dropdown_id = dropdown["props"]["id"]
            if dropdown_id["index"] != delete_index:
                updated.append(child)

        # 行が0にならないようにする
        if not updated:
            return current_children, row_count

        return updated, row_count

    return no_update, no_update


@app.callback(
    Output("graph-container", "children"),
    Input("update-btn", "n_clicks"),
    State({"type": "row-dropdown", "index": ALL}, "value"),
    prevent_initial_call=True,
)
def update_graph(n_clicks, all_values):
    """ドロップダウンの値から波形を更新する。"""
    row_groups = [v for v in all_values if v]
    if not row_groups:
        return html.Div(
            "チャンネルが選択されていません",
            style={"color": "#888", "padding": "20px"},
        )

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
