"""
app.py - Panel_y Proto #3.0 — データ間引き対応ビューア

Proto #2.0/2.1 の全機能を継承し、大規模データの描画最適化を追加。

アーキテクチャ:
  - 各行は独立した go.Figure()（make_subplotsは使わない）
  - hover-x-store / xaxis-range-store を介して全行同期
  - pattern-matching callback（ALL）で動的行数に対応
  - Min-Max envelope による描画最適化（閾値超過時に自動適用）
  - ズーム連動でenvelope ↔ 生データを自動切り替え

起動:
  python app.py → http://localhost:8050
"""

from pathlib import Path

import numpy as np
import pandas as pd
import plotly.graph_objects as go
import dash
from dash import dcc, html, Input, Output, State, ALL, ctx, no_update, Patch

# ---------------------------------------------------------------------------
# 定数
# ---------------------------------------------------------------------------
GRAPH_HEIGHT = 180
DATA_DIR = Path(__file__).parent / "data"
DECIMATE_THRESHOLD = 5000  # この点数を超えたらMin-Max envelopeに切り替え
DARK_BG = "#1a1a1a"
ROW_BORDER = "1px solid #333"

COLOR_PALETTE = {
    "blue": "#1f77b4",
    "orange": "#ff7f0e",
    "green": "#2ca02c",
    "red": "#d62728",
    "purple": "#9467bd",
    "brown": "#8c564b",
    "pink": "#e377c2",
    "gray": "#7f7f7f",
    "yellow": "#bcbd22",
    "cyan": "#17becf",
}
COLOR_OPTIONS = [{"label": name, "value": code} for name, code in COLOR_PALETTE.items()]
TRACE_COLORS = list(COLOR_PALETTE.values())

BTN_STYLE = {
    "padding": "6px 16px",
    "backgroundColor": "#444",
    "color": "white",
    "border": "1px solid #666",
    "borderRadius": "4px",
    "cursor": "pointer",
    "fontSize": "13px",
}

LABEL_COL_STYLE = {
    "width": "200px",
    "minWidth": "80px",
    "padding": "8px 14px",
    "display": "flex",
    "flexDirection": "column",
    "justifyContent": "center",
    "borderRight": ROW_BORDER,
    "fontFamily": "monospace",
    "resize": "horizontal",
    "overflow": "hidden",
}

# ---------------------------------------------------------------------------
# グローバル状態
# ---------------------------------------------------------------------------
df: pd.DataFrame | None = None
channels: list[str] = []

# ---------------------------------------------------------------------------
# ヘルパー関数
# ---------------------------------------------------------------------------


def minmax_envelope(time: np.ndarray, data: np.ndarray, n_buckets: int):
    """Min-Max envelope: バケットごとにmin/maxを保持し、ピークを欠落させずに圧縮する。

    Returns (t_env, d_min, d_max) — 各バケットの代表時刻, 最小値, 最大値。
    """
    n = len(data)
    bucket_size = n / n_buckets
    t_env = np.empty(n_buckets)
    d_min = np.empty(n_buckets)
    d_max = np.empty(n_buckets)
    for i in range(n_buckets):
        i0 = int(i * bucket_size)
        i1 = int((i + 1) * bucket_size)
        if i1 <= i0:
            i1 = i0 + 1
        seg = data[i0:i1]
        t_env[i] = time[i0]
        d_min[i] = seg.min()
        d_max[i] = seg.max()
    return t_env, d_min, d_max



def make_row_fig(
    chs: list[str],
    show_xaxis: bool = False,
    ymin: float | None = None,
    ymax: float | None = None,
    lock_y: bool = False,
    step_chs: set[str] | None = None,
    ch_styles: dict[str, dict] | None = None,
    x_range: list | None = None,
) -> go.Figure:
    """行1つ分の Figure を生成する。点数に応じてenvelope/生データを自動切り替え。"""
    fig = go.Figure()

    # 表示範囲のスライス
    time_arr = df["time"].values
    if x_range and len(x_range) == 2:
        mask = (time_arr >= x_range[0]) & (time_arr <= x_range[1])
    else:
        mask = np.ones(len(time_arr), dtype=bool)
    t_vis = time_arr[mask]
    n_vis = len(t_vis)

    for j, ch in enumerate(chs):
        if ch in df.columns:
            style = ch_styles.get(ch, {}) if ch_styles else {}
            color = style.get("color", TRACE_COLORS[j % len(TRACE_COLORS)])
            width = style.get("width", 1)
            is_step = step_chs and ch in step_chs
            d_vis = df[ch].values[mask]

            if n_vis > DECIMATE_THRESHOLD and not is_step:
                # Min-Max envelope
                n_buckets = DECIMATE_THRESHOLD // 2
                t_env, d_min, d_max = minmax_envelope(t_vis, d_vis, n_buckets)
                # 上限線
                fig.add_trace(go.Scatter(
                    x=t_env, y=d_max,
                    mode="lines", name=ch,
                    line=dict(width=0, color=color),
                    hoverinfo="none", showlegend=False,
                ))
                # 下限線 + fill
                # hex色をrgba(r,g,b,0.3)に変換
                if color.startswith("#") and len(color) == 7:
                    r, g, b = int(color[1:3], 16), int(color[3:5], 16), int(color[5:7], 16)
                    fill_color = f"rgba({r},{g},{b},0.3)"
                elif color.startswith("rgb("):
                    fill_color = color.replace("rgb(", "rgba(").replace(")", ",0.3)")
                else:
                    fill_color = "rgba(128,128,128,0.3)"
                fig.add_trace(go.Scatter(
                    x=t_env, y=d_min,
                    mode="lines", name=f"{ch} (envelope)",
                    line=dict(width=width, color=color),
                    fill="tonexty",
                    fillcolor=fill_color,
                    hoverinfo="none", showlegend=False,
                ))
            else:
                # 生データ
                line_shape = "hv" if is_step else "linear"
                fig.add_trace(go.Scatter(
                    x=t_vis, y=d_vis,
                    mode="lines", name=ch,
                    line=dict(width=width, color=color, shape=line_shape),
                    hoverinfo="none",
                ))

    label = " / ".join(chs)

    # Y軸設定
    yaxis_cfg = dict(title=label, automargin=False, tickformat=".3s")
    if ymin is not None and ymax is not None:
        yaxis_cfg["range"] = [ymin, ymax]
        yaxis_cfg["autorange"] = False
    elif ymin is not None:
        yaxis_cfg["range"] = [ymin, None]
    elif ymax is not None:
        yaxis_cfg["range"] = [None, ymax]
    # scrollZoom有効時、Y軸方向のズームを無効化（X軸のみズーム可能）
    if lock_y:
        yaxis_cfg["fixedrange"] = True

    fig.update_layout(
        height=GRAPH_HEIGHT,
        margin=dict(t=8, b=30 if show_xaxis else 8, l=60, r=10),
        template="plotly_dark",
        showlegend=False,
        hovermode="x",
        xaxis=dict(
            showticklabels=show_xaxis,
            title="Time [s]" if show_xaxis else "",
        ),
        yaxis=yaxis_cfg,
        paper_bgcolor="rgba(0,0,0,0)",
        plot_bgcolor="#1e1e1e",
    )
    return fig


def waveform_row(
    row_index: int,
    chs: list[str],
    is_last: bool,
    scroll_zoom: bool = False,
    ymin: float | None = None,
    ymax: float | None = None,
    step_chs: set[str] | None = None,
    ch_styles: dict[str, dict] | None = None,
    x_range: list | None = None,
) -> html.Div:
    """波形1行分のレイアウト（ラベル列 + グラフ）を生成する。"""
    label = " / ".join(chs)
    return html.Div([
        html.Div([
            html.Span(label, style={
                "color": "#aaa",
                "fontSize": "12px",
                "fontWeight": "bold",
            }),
            html.Span("---", id={"type": "wf-val", "row": row_index}, style={
                "color": "#7ec8e3",
                "fontSize": "16px",
                "marginTop": "6px",
            }),
        ], style=LABEL_COL_STYLE),

        dcc.Graph(
            id={"type": "wf-graph", "row": row_index},
            figure=make_row_fig(
                chs, show_xaxis=is_last, ymin=ymin, ymax=ymax,
                lock_y=scroll_zoom, step_chs=step_chs, ch_styles=ch_styles,
                x_range=x_range,
            ),
            config={
                "scrollZoom": scroll_zoom,
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


YAXIS_INPUT_STYLE = {
    "width": "70px",
    "padding": "4px 6px",
    "backgroundColor": "#2a2a2a",
    "color": "#eee",
    "border": "1px solid #555",
    "borderRadius": "4px",
    "fontFamily": "monospace",
    "fontSize": "12px",
}


def make_ch_settings(ch_list: list[str]) -> list:
    """チャンネルごとの色・太さ設定UIを生成する。"""
    rows = []
    for i, ch in enumerate(ch_list):
        default_color = TRACE_COLORS[i % len(TRACE_COLORS)]
        rows.append(html.Div([
            html.Span(ch, style={
                "color": "#aaa", "fontSize": "12px", "fontFamily": "monospace",
                "minWidth": "100px", "marginRight": "8px",
            }),
            html.Span("色:", style={"color": "#888", "fontSize": "11px", "marginRight": "4px"}),
            dcc.Dropdown(
                id={"type": "ch-color", "ch": ch},
                options=COLOR_OPTIONS,
                value=default_color,
                clearable=False,
                style={"width": "100px"},
            ),
            html.Span("太さ:", style={
                "color": "#888", "fontSize": "11px", "marginLeft": "12px", "marginRight": "4px",
            }),
            dcc.Input(
                id={"type": "ch-width", "ch": ch},
                type="number",
                value=1,
                min=0.5,
                max=5,
                step=0.5,
                style={
                    "width": "50px", "padding": "2px 4px",
                    "backgroundColor": "#2a2a2a", "color": "#eee",
                    "border": "1px solid #555", "borderRadius": "4px",
                    "fontFamily": "monospace", "fontSize": "12px",
                },
            ),
        ], style={
            "display": "flex", "alignItems": "center", "padding": "3px 0",
        }))
    return rows


def make_dropdown_row(
    row_index: int,
    selected: list[str] | None = None,
    step_selected: list[str] | None = None,
) -> html.Div:
    """1行分のドロップダウン行UIを生成する。"""
    ch_options = [{"label": ch, "value": ch} for ch in channels]
    return html.Div(
        [
            html.Span(
                f"行{row_index + 1}",
                style={
                    "color": "#aaa",
                    "fontSize": "12px",
                    "minWidth": "28px",
                    "marginRight": "6px",
                    "alignSelf": "center",
                },
            ),
            dcc.Dropdown(
                id={"type": "row-dropdown", "index": row_index},
                options=ch_options,
                value=selected or [],
                multi=True,
                placeholder="ch選択...",
                style={"flex": "2", "minWidth": "140px"},
            ),
            # step表示チャンネル選択
            html.Div([
                html.Span("step:", style={
                    "color": "#888", "fontSize": "11px", "marginRight": "4px",
                    "whiteSpace": "nowrap",
                }),
                dcc.Dropdown(
                    id={"type": "step-channels", "index": row_index},
                    options=ch_options,
                    value=step_selected or [],
                    multi=True,
                    placeholder="ch...",
                    style={"flex": "1", "minWidth": "100px"},
                ),
            ], style={
                "display": "flex",
                "alignItems": "center",
                "marginLeft": "8px",
                "flex": "1",
            }),
            # Y軸範囲指定
            html.Div([
                html.Span("Y:", style={
                    "color": "#888", "fontSize": "11px", "marginRight": "4px",
                }),
                dcc.Input(
                    id={"type": "ymin-input", "index": row_index},
                    type="number",
                    placeholder="min",
                    debounce=True,
                    style=YAXIS_INPUT_STYLE,
                ),
                html.Span("~", style={
                    "color": "#888", "margin": "0 4px",
                }),
                dcc.Input(
                    id={"type": "ymax-input", "index": row_index},
                    type="number",
                    placeholder="max",
                    debounce=True,
                    style=YAXIS_INPUT_STYLE,
                ),
            ], style={
                "display": "flex",
                "alignItems": "center",
                "marginLeft": "8px",
            }),
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


def list_path_suggestions(path_str: str) -> list:
    """パス文字列からファイル/ディレクトリの候補を返す。"""
    if not path_str:
        path_str = str(DATA_DIR) + "/"

    path = Path(path_str)

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

        # ─ スケールロック ─
        html.Button("🔒 スケール固定", id="scroll-zoom-btn", n_clicks=0, style={
            "marginLeft": "16px", "padding": "6px 12px",
            "backgroundColor": "#4fc3f7", "color": "#000",
            "border": "1px solid #4fc3f7", "borderRadius": "4px",
            "cursor": "pointer", "whiteSpace": "nowrap", "fontWeight": "bold",
        }),

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

        # 右端
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

    # ━━━ 行定義エリア ━━━
    html.Div([
        html.Div([
            html.Button("+ 行追加", id="add-row-btn", n_clicks=0, style=BTN_STYLE),
            html.Button("更新 (Ctrl+Enter)", id="update-btn", n_clicks=0, style={
                **BTN_STYLE,
                "marginLeft": "8px",
                "backgroundColor": "#2a6496",
            }),
        ], style={"display": "flex", "marginBottom": "8px"}),
        html.Div(id="rows-container", children=[]),
    ], style={
        "padding": "12px 16px",
        "maxWidth": "900px",
        "borderBottom": ROW_BORDER,
    }),

    # ━━━ チャンネル設定パネル（折りたたみ） ━━━
    html.Details([
        html.Summary("チャンネル設定（色・太さ）", style={
            "color": "#aaa", "fontSize": "12px", "cursor": "pointer",
            "padding": "6px 0", "fontFamily": "monospace",
        }),
        html.Div(id="ch-settings-container", children=[]),
    ], style={
        "padding": "4px 16px",
        "borderBottom": ROW_BORDER,
    }),

    # ━━━ 差分表示パネル ━━━
    html.Div(id="delta-panel", style={"display": "none"}),

    # ━━━ 波形表示コンテナ ━━━
    html.Div(id="waveform-container"),

    # ━━━ キーボードショートカット用リスナー ━━━
    html.Div(id="keyboard-listener", tabIndex="0", style={
        "position": "fixed", "width": "0", "height": "0", "overflow": "hidden",
    }),

    # ━━━ Store ━━━
    dcc.Store(id="row-count", data=0),
    dcc.Store(id="row-groups-store", data=[]),
    dcc.Store(id="data-store"),
    dcc.Store(id="hover-x-store"),
    dcc.Store(id="xaxis-range-store"),
    dcc.Store(id="cursor-a-store"),
    dcc.Store(id="cursor-b-store"),
    dcc.Store(id="measure-mode", data=False),
    dcc.Store(id="scroll-zoom-store", data=False),  # デフォルトOFF（ページスクロール優先）

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
# Callback: ファイル読み込み → 行UIを初期生成
# ---------------------------------------------------------------------------
@app.callback(
    Output("data-store", "data"),
    Output("load-status", "children"),
    Output("rows-container", "children", allow_duplicate=True),
    Output("row-count", "data", allow_duplicate=True),
    Output("cursor-a-store", "data", allow_duplicate=True),
    Output("cursor-b-store", "data", allow_duplicate=True),
    Output("ch-settings-container", "children"),
    Input("load-btn", "n_clicks"),
    State("file-path-input", "value"),
    prevent_initial_call=True,
)
def load_file(n_clicks, file_path):
    if not n_clicks or not file_path:
        return (no_update,) * 7

    path = Path(file_path)

    if not path.exists():
        return no_update, "❌ ファイルが見つかりません", no_update, no_update, no_update, no_update, no_update
    if path.is_dir():
        return no_update, "❌ ディレクトリです", no_update, no_update, no_update, no_update, no_update
    if path.suffix.lower() != ".parquet":
        return no_update, "❌ .parquet を指定してください", no_update, no_update, no_update, no_update, no_update

    global df, channels
    df = pd.read_parquet(path)
    channels = [col for col in df.columns if col != "time"]

    if not channels:
        df = None
        channels = []
        return no_update, "❌ 波形チャンネルがありません", no_update, no_update, no_update, no_update, no_update

    n = len(df)
    ts = df["time"].iloc[1] - df["time"].iloc[0]
    status = f"✓ {path.name} ({len(channels)} ch, {n:,} 点, {1/ts:,.0f} Hz)"

    # 初期行: チャンネルごとに1行ずつ（最大8行まで）
    initial_channels = channels[:8]
    row_divs = [make_dropdown_row(i, [ch]) for i, ch in enumerate(initial_channels)]

    # チャンネル設定パネル生成
    ch_settings = make_ch_settings(channels)

    return (
        {"path": str(path), "channels": channels},
        status,
        row_divs,
        len(initial_channels),
        None,
        None,
        ch_settings,
    )


# ---------------------------------------------------------------------------
# Callback: 行の追加・削除
# ---------------------------------------------------------------------------
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
        new_row = make_dropdown_row(row_count)
        current_children.append(new_row)
        return current_children, row_count + 1

    if isinstance(triggered, dict) and triggered.get("type") == "row-delete":
        delete_index = triggered["index"]
        updated = []
        for child in current_children:
            row_div_children = child["props"]["children"]
            dropdown = row_div_children[1]
            dropdown_id = dropdown["props"]["id"]
            if dropdown_id["index"] != delete_index:
                updated.append(child)
        if not updated:
            return current_children, row_count
        return updated, row_count

    return no_update, no_update


# ---------------------------------------------------------------------------
# Callback: スケールロック（scrollZoom）トグル
# ---------------------------------------------------------------------------
@app.callback(
    Output("scroll-zoom-store", "data"),
    Output("scroll-zoom-btn", "children"),
    Output("scroll-zoom-btn", "style"),
    Input("scroll-zoom-btn", "n_clicks"),
    prevent_initial_call=True,
)
def toggle_scroll_zoom(n_clicks):
    # 偶数クリック=ロック（OFF）、奇数=解除（ON）
    zoom_on = n_clicks % 2 == 1
    if zoom_on:
        label = "🔓 スケール解除"
        style = {
            "marginLeft": "16px", "padding": "6px 12px",
            "backgroundColor": "#333", "color": "#aaa",
            "border": "1px solid #555", "borderRadius": "4px",
            "cursor": "pointer", "whiteSpace": "nowrap",
        }
    else:
        label = "🔒 スケール固定"
        style = {
            "marginLeft": "16px", "padding": "6px 12px",
            "backgroundColor": "#4fc3f7", "color": "#000",
            "border": "1px solid #4fc3f7", "borderRadius": "4px",
            "cursor": "pointer", "whiteSpace": "nowrap", "fontWeight": "bold",
        }
    return zoom_on, label, style


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
    cursor_a = no_update if is_on else None
    cursor_b = no_update if is_on else None
    return is_on, style, cursor_a, cursor_b


# ---------------------------------------------------------------------------
# Callback: 更新ボタン → 波形行を動的生成
# ---------------------------------------------------------------------------
@app.callback(
    Output("waveform-container", "children"),
    Output("row-groups-store", "data"),
    Input("update-btn", "n_clicks"),
    Input("scroll-zoom-store", "data"),
    State({"type": "row-dropdown", "index": ALL}, "value"),
    State({"type": "ymin-input", "index": ALL}, "value"),
    State({"type": "ymax-input", "index": ALL}, "value"),
    State({"type": "step-channels", "index": ALL}, "value"),
    State({"type": "ch-color", "ch": ALL}, "value"),
    State({"type": "ch-width", "ch": ALL}, "value"),
    State({"type": "ch-color", "ch": ALL}, "id"),
    State("xaxis-range-store", "data"),
    prevent_initial_call=True,
)
def update_waveform_rows(
    n_clicks, scroll_zoom, all_values, all_ymin, all_ymax, all_step,
    all_colors, all_widths, all_color_ids, x_range,
):
    """ドロップダウンの値から波形行を生成する。"""
    if not all_values:
        return html.Div(
            "チャンネルが選択されていません",
            style={"color": "#888", "padding": "20px"},
        ), []

    # チャンネルスタイル辞書を構築
    ch_styles = {}
    for i, cid in enumerate(all_color_ids or []):
        ch_name = cid["ch"]
        ch_styles[ch_name] = {
            "color": all_colors[i] if i < len(all_colors) else TRACE_COLORS[0],
            "width": all_widths[i] if i < len(all_widths) and all_widths[i] else 1,
        }

    # 空でない行だけ抽出（対応するymin/ymax/stepも連動）
    row_groups = []
    ymin_list = []
    ymax_list = []
    step_list = []
    for i, v in enumerate(all_values):
        if v:
            row_groups.append(v)
            ymin_list.append(all_ymin[i] if i < len(all_ymin) else None)
            ymax_list.append(all_ymax[i] if i < len(all_ymax) else None)
            raw_step = all_step[i] if i < len(all_step) else None
            step_list.append(set(raw_step) if raw_step and isinstance(raw_step, list) else set())

    if not row_groups or df is None:
        return html.Div(
            "チャンネルが選択されていません",
            style={"color": "#888", "padding": "20px"},
        ), []

    rows = [
        # ヘッダー行
        html.Div([
            html.Div("Channel / Value", style={
                "width": "200px", "padding": "4px 14px",
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

    for i, group in enumerate(row_groups):
        rows.append(waveform_row(
            i, group,
            is_last=(i == len(row_groups) - 1),
            scroll_zoom=scroll_zoom,
            ymin=ymin_list[i],
            ymax=ymax_list[i],
            step_chs=step_list[i],
            ch_styles=ch_styles,
            x_range=x_range,
        ))

    return rows, row_groups


# ---------------------------------------------------------------------------
# Callback: ホバー X 座標を Store に保存
# ---------------------------------------------------------------------------
@app.callback(
    Output("hover-x-store", "data"),
    Input({"type": "wf-graph", "row": ALL}, "hoverData"),
    prevent_initial_call=True,
)
def store_hover_x(hover_datas):
    for t in ctx.triggered:
        val = t.get("value")
        if val and isinstance(val, dict) and val.get("points"):
            return val["points"][0]["x"]
    return no_update


# ---------------------------------------------------------------------------
# Callback: ホバー値を全行に表示
# ---------------------------------------------------------------------------
@app.callback(
    Output({"type": "wf-val", "row": ALL}, "children"),
    Input("hover-x-store", "data"),
    State({"type": "wf-val", "row": ALL}, "id"),
    State("row-groups-store", "data"),
    prevent_initial_call=True,
)
def update_values(x_val, val_ids, row_groups):
    if not val_ids:
        return []
    if x_val is None or df is None or not row_groups:
        return ["---"] * len(val_ids)

    idx = (df["time"] - x_val).abs().idxmin()
    results = []
    for i, vid in enumerate(val_ids):
        if i < len(row_groups):
            group = row_groups[i]
            spans = []
            for j, ch in enumerate(group):
                if ch in df.columns:
                    color = TRACE_COLORS[j % len(TRACE_COLORS)]
                    val = df[ch].iloc[idx]
                    spans.append(html.Span(
                        f"● {val:.4g}",
                        style={"color": color, "marginRight": "6px", "fontSize": "13px"},
                    ))
            results.append(html.Div(spans) if spans else "---")
        else:
            results.append("---")
    return results


# ---------------------------------------------------------------------------
# Callback: ズーム・パン → Store
# ---------------------------------------------------------------------------
@app.callback(
    Output("xaxis-range-store", "data"),
    Input({"type": "wf-graph", "row": ALL}, "relayoutData"),
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
    Output({"type": "wf-graph", "row": ALL}, "figure"),
    Input("hover-x-store", "data"),
    Input("xaxis-range-store", "data"),
    Input("cursor-a-store", "data"),
    Input("cursor-b-store", "data"),
    State({"type": "wf-graph", "row": ALL}, "id"),
    State("row-groups-store", "data"),
    State({"type": "step-channels", "index": ALL}, "value"),
    State({"type": "ch-color", "ch": ALL}, "value"),
    State({"type": "ch-width", "ch": ALL}, "value"),
    State({"type": "ch-color", "ch": ALL}, "id"),
    State("scroll-zoom-store", "data"),
    State({"type": "ymin-input", "index": ALL}, "value"),
    State({"type": "ymax-input", "index": ALL}, "value"),
    prevent_initial_call=True,
)
def update_graphs(
    x_hover, xaxis_range, cursor_a, cursor_b, graph_ids,
    row_groups, all_step, all_colors, all_widths, all_color_ids,
    scroll_zoom, all_ymin, all_ymax,
):
    if not graph_ids:
        return []

    # ズーム変更がトリガーの場合、表示点数に応じてトレースを再描画
    triggered = [t["prop_id"] for t in ctx.triggered]
    zoom_changed = any("xaxis-range-store" in t for t in triggered)

    if zoom_changed and row_groups and df is not None:
        # チャンネルスタイル辞書を構築
        ch_styles = {}
        for i, cid in enumerate(all_color_ids or []):
            ch_name = cid["ch"]
            ch_styles[ch_name] = {
                "color": all_colors[i] if i < len(all_colors) else TRACE_COLORS[0],
                "width": all_widths[i] if i < len(all_widths) and all_widths[i] else 1,
            }

        results = []
        for i, gid in enumerate(graph_ids):
            row_idx = i
            if row_idx < len(row_groups):
                group = row_groups[row_idx]
                raw_step = all_step[row_idx] if row_idx < len(all_step) else None
                step_chs = set(raw_step) if raw_step and isinstance(raw_step, list) else set()
                y_lo = all_ymin[row_idx] if row_idx < len(all_ymin) else None
                y_hi = all_ymax[row_idx] if row_idx < len(all_ymax) else None

                fig = make_row_fig(
                    group,
                    show_xaxis=(row_idx == len(row_groups) - 1),
                    ymin=y_lo, ymax=y_hi,
                    lock_y=scroll_zoom,
                    step_chs=step_chs,
                    ch_styles=ch_styles,
                    x_range=xaxis_range,
                )
                # shapes を追加
                shapes = _build_shapes(x_hover, cursor_a, cursor_b)
                fig.update_layout(shapes=shapes)
                if xaxis_range:
                    fig.update_layout(xaxis=dict(range=xaxis_range, autorange=False))
                results.append(fig)
            else:
                results.append(no_update)
        return results

    # 通常パス: Patch() でレイアウトのみ更新
    results = []
    shapes = _build_shapes(x_hover, cursor_a, cursor_b)

    for _ in graph_ids:
        p = Patch()
        p["layout"]["shapes"] = shapes

        if xaxis_range:
            p["layout"]["xaxis"]["range"] = xaxis_range
            p["layout"]["xaxis"]["autorange"] = False
        else:
            p["layout"]["xaxis"]["autorange"] = True

        results.append(p)

    return results


def _build_shapes(x_hover, cursor_a, cursor_b):
    """スパイクライン・カーソル線の shapes リストを生成する。"""
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
    return shapes


# ---------------------------------------------------------------------------
# Callback: カーソル設置（計測モード時のクリック交互方式）
# ---------------------------------------------------------------------------
@app.callback(
    Output("cursor-a-store", "data"),
    Output("cursor-b-store", "data"),
    Input({"type": "wf-graph", "row": ALL}, "clickData"),
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
    State("row-groups-store", "data"),
)
def update_delta_panel(cursor_a, cursor_b, row_groups):
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

    # 差分計算 + 区間解析
    if cursor_a is not None and cursor_b is not None and df is not None:
        dt = cursor_b - cursor_a
        children.append(html.Div(
            f"Δt = {abs(dt) * 1000:.3f} ms   ({1/abs(dt):.4g} Hz)" if dt != 0 else "Δt = 0 ms",
            style={"marginTop": "4px", "color": "#fff", "fontWeight": "bold"},
        ))

        idx_a = (df["time"] - cursor_a).abs().idxmin()
        idx_b = (df["time"] - cursor_b).abs().idxmin()
        i_lo, i_hi = min(idx_a, idx_b), max(idx_a, idx_b) + 1

        # 全行の全チャンネルを対象（重複除去・順序維持）
        seen = set()
        unique_channels = []
        if row_groups:
            for group in row_groups:
                for ch in group:
                    if ch not in seen:
                        seen.add(ch)
                        unique_channels.append(ch)

        # テーブルヘッダー
        hdr_style = {"padding": "2px 8px", "color": "#888", "fontSize": "11px",
                     "borderBottom": "1px solid #444", "whiteSpace": "nowrap"}
        val_style = {"padding": "2px 8px", "fontSize": "12px", "whiteSpace": "nowrap"}

        table_rows = [html.Tr([
            html.Th("ch", style=hdr_style),
            html.Th("A", style=hdr_style),
            html.Th("B", style=hdr_style),
            html.Th("Δ", style=hdr_style),
            html.Th("Mean", style=hdr_style),
            html.Th("Max", style=hdr_style),
            html.Th("Min", style=hdr_style),
            html.Th("P-P", style=hdr_style),
            html.Th("RMS", style=hdr_style),
            html.Th("Slope/s", style=hdr_style),
        ])]

        for ch in unique_channels:
            if ch in df.columns:
                seg = df[ch].iloc[i_lo:i_hi]
                va = df[ch].iloc[idx_a]
                vb = df[ch].iloc[idx_b]
                v_mean = seg.mean()
                v_max = seg.max()
                v_min = seg.min()
                v_pp = v_max - v_min
                v_rms = np.sqrt((seg ** 2).mean())
                v_slope = (vb - va) / dt if dt != 0 else 0

                table_rows.append(html.Tr([
                    html.Td(ch, style={**val_style, "color": "#aaa"}),
                    html.Td(f"{va:.4g}", style={**val_style, "color": "#4fc3f7"}),
                    html.Td(f"{vb:.4g}", style={**val_style, "color": "#ef5350"}),
                    html.Td(f"{vb - va:.4g}", style=val_style),
                    html.Td(f"{v_mean:.4g}", style=val_style),
                    html.Td(f"{v_max:.4g}", style=val_style),
                    html.Td(f"{v_min:.4g}", style=val_style),
                    html.Td(f"{v_pp:.4g}", style=val_style),
                    html.Td(f"{v_rms:.4g}", style=val_style),
                    html.Td(f"{v_slope:.4g}", style=val_style),
                ]))

        if len(table_rows) > 1:
            children.append(html.Table(
                table_rows,
                style={"marginTop": "6px", "borderCollapse": "collapse"},
            ))

    return children, base_style


# ---------------------------------------------------------------------------
# Clientside callback: Ctrl+Enter → 更新ボタンをクリック
# ---------------------------------------------------------------------------
app.clientside_callback(
    """
    function() {
        if (!window._panely_keydown_registered) {
            document.addEventListener('keydown', function(e) {
                if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
                    e.preventDefault();
                    var btn = document.getElementById('update-btn');
                    if (btn) btn.click();
                }
            });
            window._panely_keydown_registered = true;
        }
        return window.dash_clientside.no_update;
    }
    """,
    Output("keyboard-listener", "style"),
    Input("keyboard-listener", "id"),
)


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------
if __name__ == "__main__":
    DATA_DIR.mkdir(exist_ok=True)
    print(f"データディレクトリ: {DATA_DIR}")
    print(f"起動: http://localhost:8050")
    app.run(debug=True)
