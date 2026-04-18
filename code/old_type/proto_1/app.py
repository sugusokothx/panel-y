"""
app.py - Panel_y Proto #1 — 実用ビューア

機能:
  - ファイル選択（パス入力 + 補完）
  - チャンネル表示/非表示（ドロップダウン型マルチセレクト）
  - 行レイアウト + ホバー値 + スパイクライン + ズーム同期（#0.2 継承）
  - カーソル差分計算（計測モード）

起動:
  python app.py → http://localhost:8050
"""

from pathlib import Path

import pandas as pd
import plotly.graph_objects as go
import dash
from dash import dcc, html, Input, Output, State, Patch, ctx, no_update, ALL

# ---------------------------------------------------------------------------
# 定数
# ---------------------------------------------------------------------------
GRAPH_HEIGHT = 180
DATA_DIR = Path(__file__).parent / "data"
DARK_BG = "#1a1a1a"
ROW_BORDER = "1px solid #333"

LABEL_COL_STYLE = {
    "width": "140px",
    "minWidth": "140px",
    "padding": "8px 14px",
    "display": "flex",
    "flexDirection": "column",
    "justifyContent": "center",
    "borderRight": ROW_BORDER,
    "fontFamily": "monospace",
}

# ---------------------------------------------------------------------------
# グローバル状態（サーバーサイド — サイズ制限なし）
# ---------------------------------------------------------------------------
df: pd.DataFrame | None = None

# ---------------------------------------------------------------------------
# ヘルパー関数
# ---------------------------------------------------------------------------


def make_channel_fig(ch: str, show_xaxis: bool = False) -> go.Figure:
    """チャンネル1つ分の Figure を生成する。"""
    fig = go.Figure()
    fig.add_trace(go.Scatter(
        x=df["time"],
        y=df[ch],
        mode="lines",
        name=ch,
        line=dict(width=1),
        hoverinfo="none",
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


def channel_row(ch: str, is_last: bool) -> html.Div:
    """チャンネル1行分のレイアウトを生成する。"""
    return html.Div([
        html.Div([
            html.Span(ch, style={
                "color": "#aaa",
                "fontSize": "12px",
                "fontWeight": "bold",
            }),
            html.Span("---", id={"type": "wf-val", "channel": ch}, style={
                "color": "#7ec8e3",
                "fontSize": "20px",
                "marginTop": "6px",
            }),
        ], style=LABEL_COL_STYLE),

        dcc.Graph(
            id={"type": "wf-graph", "channel": ch},
            figure=make_channel_fig(ch, show_xaxis=is_last),
            config={
                "scrollZoom": True,
                "displayModeBar": "hover",
                "modeBarButtonsToRemove": ["select2d", "lasso2d"],
            },
            style={"flex": "1", "height": f"{GRAPH_HEIGHT}px"},
        ),
    ], style={
        "display": "flex",
        "alignItems": "stretch",
        "borderBottom": ROW_BORDER,
        "backgroundColor": DARK_BG,
    })


def list_path_suggestions(path_str: str) -> list:
    """パス文字列からファイル/ディレクトリの候補を返す。"""
    if not path_str:
        path_str = str(DATA_DIR) + "/"

    path = Path(path_str)

    # 既に有効な .parquet ファイルを指している場合は候補なし
    if path.is_file() and path.suffix.lower() == ".parquet":
        return []

    if path_str.endswith("/") and path.is_dir():
        parent, prefix = path, ""
    elif path.parent.is_dir():
        parent, prefix = path.parent, path.name
    else:
        return []

    btn_style_base = {
        "display": "block",
        "width": "100%",
        "textAlign": "left",
        "padding": "6px 12px",
        "border": "none",
        "borderBottom": "1px solid #333",
        "backgroundColor": "#2a2a2a",
        "cursor": "pointer",
        "fontFamily": "monospace",
        "fontSize": "13px",
    }

    candidates = []
    try:
        for p in sorted(parent.iterdir()):
            if p.name.startswith("."):
                continue
            if prefix and not p.name.lower().startswith(prefix.lower()):
                continue
            if p.is_dir():
                candidates.append(html.Button(
                    f"📁 {p.name}/",
                    id={"type": "suggestion", "path": str(p) + "/"},
                    n_clicks=0,
                    style={**btn_style_base, "color": "#ccc"},
                ))
            elif p.suffix.lower() == ".parquet":
                candidates.append(html.Button(
                    f"📄 {p.name}",
                    id={"type": "suggestion", "path": str(p)},
                    n_clicks=0,
                    style={**btn_style_base, "color": "#8bc34a"},
                ))
    except PermissionError:
        pass

    return candidates[:30]


# ---------------------------------------------------------------------------
# Dash アプリ
# ---------------------------------------------------------------------------
app = dash.Dash(__name__, suppress_callback_exceptions=True)

app.layout = html.Div([

    # ━━━ ヘッダーバー ━━━
    html.Div([
        html.H2("PanelY", style={
            "margin": "0 16px 0 0", "color": "white",
            "fontFamily": "sans-serif", "fontSize": "18px",
            "whiteSpace": "nowrap",
        }),

        # ─ ファイル選択 ─
        html.Div([
            html.Div([
                dcc.Input(
                    id="file-path-input",
                    type="text",
                    value=str(DATA_DIR) + "/",
                    placeholder="Parquet ファイルパスを入力...",
                    debounce=False,
                    style={
                        "width": "420px", "padding": "6px 10px",
                        "backgroundColor": "#2a2a2a", "color": "#eee",
                        "border": "1px solid #555", "borderRadius": "4px",
                        "fontFamily": "monospace", "fontSize": "13px",
                    },
                ),
                html.Div(id="file-suggestions", style={"display": "none"}),
            ], style={"position": "relative"}),

            html.Button("読み込み", id="load-btn", n_clicks=0, style={
                "marginLeft": "8px", "padding": "6px 16px",
                "backgroundColor": "#4fc3f7", "color": "#000",
                "border": "none", "borderRadius": "4px",
                "cursor": "pointer", "fontWeight": "bold",
            }),
        ], style={"display": "flex", "alignItems": "center"}),

        # ─ チャンネル選択 ─
        html.Div([
            dcc.Dropdown(
                id="ch-dropdown",
                options=[],
                value=[],
                multi=True,
                searchable=True,
                placeholder="ch を選択...",
                style={"width": "300px"},
            ),
        ], style={"marginLeft": "16px"}),

        # ─ 計測モードトグル ─
        html.Button("📏 計測", id="measure-toggle-btn", n_clicks=0, style={
            "marginLeft": "16px", "padding": "6px 12px",
            "backgroundColor": "#333", "color": "#aaa",
            "border": "1px solid #555", "borderRadius": "4px",
            "cursor": "pointer", "whiteSpace": "nowrap",
        }),

        # ─ ステータス ─
        html.Span(id="load-status", style={
            "marginLeft": "16px", "color": "#8bc34a",
            "fontFamily": "monospace", "fontSize": "12px",
            "whiteSpace": "nowrap",
        }),

        # 右端に producted by YTDC
        html.Span("producted by YTDC", style={
            "marginLeft": "auto", "color": "#666",
            "fontFamily": "sans-serif", "fontSize": "11px",
            "whiteSpace": "nowrap",
        }),
    ], style={
        "display": "flex", "alignItems": "center",
        "padding": "10px 16px", "backgroundColor": "#222",
        "borderBottom": ROW_BORDER, "flexWrap": "wrap", "gap": "8px",
    }),

    # ━━━ 差分表示パネル ━━━
    html.Div(id="delta-panel", style={"display": "none"}),

    # ━━━ 波形表示コンテナ ━━━
    html.Div(id="waveform-container"),

    # ━━━ Store ━━━
    dcc.Store(id="hover-x-store"),
    dcc.Store(id="xaxis-range-store"),
    dcc.Store(id="data-store"),
    dcc.Store(id="cursor-a-store"),
    dcc.Store(id="cursor-b-store"),
    dcc.Store(id="measure-mode", data=False),

], style={"backgroundColor": DARK_BG, "minHeight": "100vh"})


# ---------------------------------------------------------------------------
# Callback: ファイルパス候補を表示
# ---------------------------------------------------------------------------
@app.callback(
    Output("file-suggestions", "children"),
    Output("file-suggestions", "style"),
    Input("file-path-input", "value"),
)
def suggest_files(path_str):
    suggestions = list_path_suggestions(path_str or "")
    if not suggestions:
        return [], {"display": "none"}
    return suggestions, {
        "position": "absolute", "top": "100%", "left": "0",
        "width": "420px", "zIndex": "1000",
        "maxHeight": "300px", "overflowY": "auto",
        "backgroundColor": "#2a2a2a",
        "border": "1px solid #555", "borderRadius": "0 0 4px 4px",
        "display": "block",
    }


# ---------------------------------------------------------------------------
# Callback: 候補クリック → パス更新
# ---------------------------------------------------------------------------
@app.callback(
    Output("file-path-input", "value"),
    Input({"type": "suggestion", "path": ALL}, "n_clicks"),
    prevent_initial_call=True,
)
def on_suggestion_click(n_clicks_list):
    if not any(n for n in n_clicks_list if n):
        return no_update
    triggered = ctx.triggered_id
    if triggered and isinstance(triggered, dict):
        return triggered["path"]
    return no_update


# ---------------------------------------------------------------------------
# Callback: ファイル読み込み
# ---------------------------------------------------------------------------
@app.callback(
    Output("data-store", "data"),
    Output("ch-dropdown", "options"),
    Output("ch-dropdown", "value"),
    Output("load-status", "children"),
    Output("cursor-a-store", "data", allow_duplicate=True),
    Output("cursor-b-store", "data", allow_duplicate=True),
    Input("load-btn", "n_clicks"),
    State("file-path-input", "value"),
    prevent_initial_call=True,
)
def load_file(n_clicks, file_path):
    if not n_clicks or not file_path:
        return (no_update,) * 6

    path = Path(file_path)

    if not path.exists():
        return no_update, no_update, no_update, "❌ ファイルが見つかりません", no_update, no_update
    if path.is_dir():
        return no_update, no_update, no_update, "❌ ディレクトリです", no_update, no_update
    if path.suffix.lower() != ".parquet":
        return no_update, no_update, no_update, "❌ .parquet を指定してください", no_update, no_update

    global df
    df = pd.read_parquet(path)
    channels = [col for col in df.columns if col != "time"]

    if not channels:
        df = None
        return no_update, no_update, no_update, "❌ 波形チャンネルがありません", no_update, no_update

    ch_options = [{"label": ch, "value": ch} for ch in channels]
    n = len(df)
    ts = df["time"].iloc[1] - df["time"].iloc[0]

    return (
        {"path": str(path), "channels": channels},
        ch_options,
        [],
        f"✓ {path.name} ({len(channels)} ch, {n:,} 点, {1/ts:,.0f} Hz)",
        None,
        None,
    )


# ---------------------------------------------------------------------------
# Callback: 計測モードトグル
# ---------------------------------------------------------------------------
@app.callback(
    Output("measure-mode", "data"),
    Output("measure-toggle-btn", "style"),
    Output("cursor-a-store", "data", allow_duplicate=True),
    Output("cursor-b-store", "data", allow_duplicate=True),
    Input("measure-toggle-btn", "n_clicks"),
    prevent_initial_call=True,
)
def toggle_measure(n_clicks):
    is_on = n_clicks % 2 == 1
    style = {
        "marginLeft": "16px", "padding": "6px 12px",
        "borderRadius": "4px", "cursor": "pointer",
        "whiteSpace": "nowrap",
    }
    if is_on:
        style.update({
            "backgroundColor": "#4fc3f7", "color": "#000",
            "border": "1px solid #4fc3f7", "fontWeight": "bold",
        })
    else:
        style.update({
            "backgroundColor": "#333", "color": "#aaa",
            "border": "1px solid #555",
        })
    # 計測 OFF → カーソルリセット
    cursor_a = no_update if is_on else None
    cursor_b = no_update if is_on else None
    return is_on, style, cursor_a, cursor_b


# ---------------------------------------------------------------------------
# Callback: チャンネル選択 → 波形行を動的生成
# ---------------------------------------------------------------------------
@app.callback(
    Output("waveform-container", "children"),
    Input("ch-dropdown", "value"),
    State("data-store", "data"),
)
def update_waveform_rows(selected_channels, data_store):
    if not selected_channels or df is None:
        return []

    rows = [
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
        ], style={
            "display": "flex", "backgroundColor": "#222",
            "borderBottom": ROW_BORDER,
        }),
    ]

    for i, ch in enumerate(selected_channels):
        rows.append(channel_row(ch, is_last=(i == len(selected_channels) - 1)))

    return rows


# ---------------------------------------------------------------------------
# Callback: ホバー X 座標を Store に保存
# ---------------------------------------------------------------------------
@app.callback(
    Output("hover-x-store", "data"),
    Input({"type": "wf-graph", "channel": ALL}, "hoverData"),
    prevent_initial_call=True,
)
def store_hover_x(hover_datas):
    for hd in hover_datas:
        if hd and hd.get("points"):
            return hd["points"][0]["x"]
    return no_update


# ---------------------------------------------------------------------------
# Callback: ホバー値を全チャンネルに表示
# ---------------------------------------------------------------------------
@app.callback(
    Output({"type": "wf-val", "channel": ALL}, "children"),
    Input("hover-x-store", "data"),
    State({"type": "wf-val", "channel": ALL}, "id"),
    prevent_initial_call=True,
)
def update_values(x_val, val_ids):
    if not val_ids:
        return []
    if x_val is None or df is None:
        return ["---"] * len(val_ids)
    idx = (df["time"] - x_val).abs().idxmin()
    return [f"{df[vid['channel']].iloc[idx]:.4f}" for vid in val_ids]


# ---------------------------------------------------------------------------
# Callback: ズーム・パン → Store
# ---------------------------------------------------------------------------
@app.callback(
    Output("xaxis-range-store", "data"),
    Input({"type": "wf-graph", "channel": ALL}, "relayoutData"),
    prevent_initial_call=True,
)
def store_xaxis_range(relayout_datas):
    for rd in relayout_datas:
        if rd is None:
            continue
        if "xaxis.range[0]" in rd:
            return [rd["xaxis.range[0]"], rd["xaxis.range[1]"]]
        if "xaxis.autorange" in rd:
            return None
    return no_update


# ---------------------------------------------------------------------------
# Callback: 全グラフ更新（スパイクライン + ズーム同期 + カーソル線）
# ---------------------------------------------------------------------------
@app.callback(
    Output({"type": "wf-graph", "channel": ALL}, "figure"),
    Input("hover-x-store", "data"),
    Input("xaxis-range-store", "data"),
    Input("cursor-a-store", "data"),
    Input("cursor-b-store", "data"),
    State({"type": "wf-graph", "channel": ALL}, "id"),
    prevent_initial_call=True,
)
def update_graphs(x_hover, xaxis_range, cursor_a, cursor_b, graph_ids):
    if not graph_ids:
        return []

    triggered = ctx.triggered_id
    results = []

    for _ in graph_ids:
        p = Patch()

        # shapes: スパイクライン + カーソル A/B
        if triggered in ("hover-x-store", "cursor-a-store", "cursor-b-store"):
            shapes = []
            if cursor_a is not None:
                shapes.append({
                    "type": "line",
                    "x0": cursor_a, "x1": cursor_a,
                    "y0": 0, "y1": 1,
                    "xref": "x", "yref": "paper",
                    "line": {"color": "#4fc3f7", "width": 2, "dash": "dash"},
                })
            if cursor_b is not None:
                shapes.append({
                    "type": "line",
                    "x0": cursor_b, "x1": cursor_b,
                    "y0": 0, "y1": 1,
                    "xref": "x", "yref": "paper",
                    "line": {"color": "#ef5350", "width": 2, "dash": "dash"},
                })
            if x_hover is not None:
                shapes.append({
                    "type": "line",
                    "x0": x_hover, "x1": x_hover,
                    "y0": 0, "y1": 1,
                    "xref": "x", "yref": "paper",
                    "line": {"color": "rgba(180,180,180,0.7)", "width": 1},
                })
            p["layout"]["shapes"] = shapes

        # X軸レンジ同期
        if triggered == "xaxis-range-store":
            if xaxis_range:
                p["layout"]["xaxis"]["range"] = xaxis_range
                p["layout"]["xaxis"]["autorange"] = False
            else:
                p["layout"]["xaxis"]["autorange"] = True

        results.append(p)

    return results


# ---------------------------------------------------------------------------
# Callback: カーソル設置（計測モード時のクリック交互方式）
# ---------------------------------------------------------------------------
@app.callback(
    Output("cursor-a-store", "data"),
    Output("cursor-b-store", "data"),
    Input({"type": "wf-graph", "channel": ALL}, "clickData"),
    State("measure-mode", "data"),
    State("cursor-a-store", "data"),
    State("cursor-b-store", "data"),
    prevent_initial_call=True,
)
def set_cursor(click_datas, measure_on, cursor_a, cursor_b):
    if not measure_on:
        return no_update, no_update

    x_val = None
    for cd in click_datas:
        if cd and cd.get("points"):
            x_val = cd["points"][0]["x"]
            break

    if x_val is None:
        return no_update, no_update

    # クリック交互: A → B → A更新(B クリア) → B → ...
    if cursor_b is not None:
        return x_val, None
    elif cursor_a is not None:
        return cursor_a, x_val
    else:
        return x_val, None


# ---------------------------------------------------------------------------
# Callback: 差分パネル更新
# ---------------------------------------------------------------------------
@app.callback(
    Output("delta-panel", "children"),
    Output("delta-panel", "style"),
    Input("cursor-a-store", "data"),
    Input("cursor-b-store", "data"),
    State("ch-dropdown", "value"),
)
def update_delta_panel(cursor_a, cursor_b, selected_channels):
    base_style = {
        "padding": "10px 16px",
        "backgroundColor": "#1e2a1e",
        "borderBottom": ROW_BORDER,
        "fontFamily": "monospace",
        "fontSize": "13px",
        "color": "#ccc",
    }

    if cursor_a is None and cursor_b is None:
        base_style["display"] = "none"
        return [], base_style

    base_style["display"] = "block"
    children = []

    # カーソル位置
    cursor_info = []
    if cursor_a is not None:
        cursor_info.append(html.Span(
            f"Cursor A: {cursor_a:.5f} s",
            style={"color": "#4fc3f7", "marginRight": "24px"},
        ))
    if cursor_b is not None:
        cursor_info.append(html.Span(
            f"Cursor B: {cursor_b:.5f} s",
            style={"color": "#ef5350", "marginRight": "24px"},
        ))
    children.append(html.Div(cursor_info))

    # 差分計算
    if cursor_a is not None and cursor_b is not None and df is not None:
        dt = cursor_b - cursor_a
        children.append(html.Div(
            f"Δt = {abs(dt) * 1000:.3f} ms",
            style={"marginTop": "4px", "color": "#fff", "fontWeight": "bold"},
        ))

        idx_a = (df["time"] - cursor_a).abs().idxmin()
        idx_b = (df["time"] - cursor_b).abs().idxmin()

        delta_items = []
        for ch in (selected_channels or []):
            va = df[ch].iloc[idx_a]
            vb = df[ch].iloc[idx_b]
            delta_items.append(html.Div(
                f"{ch}:  A={va:.4f}   B={vb:.4f}   Δ={vb - va:.4f}",
                style={"marginTop": "2px"},
            ))
        if delta_items:
            children.append(html.Div(delta_items, style={"marginTop": "4px"}))

    return children, base_style


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------
if __name__ == "__main__":
    DATA_DIR.mkdir(exist_ok=True)
    print(f"データディレクトリ: {DATA_DIR}")
    print(f"起動: http://localhost:8050")
    app.run(debug=True)
