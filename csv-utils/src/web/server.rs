use crate::web::assets::INDEX_HTML;
use anyhow::{Context, Result};
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use csv_utils_core::{
    client_view::{ClientTable, ClientView, ScrollMeta},
    column::{column_kind_from_label, numeric_repr_from_label},
    AppModel, ViewAction, ViewLayout,
};
use serde::Deserialize;
use std::{
    sync::{Arc, Mutex, RwLock},
    thread::{self, JoinHandle},
};

#[derive(Clone)]
pub struct WebServerState {
    pub model: Arc<Mutex<AppModel>>,
    pub layout: Arc<Mutex<ViewLayout>>,
    pub snapshot: Arc<RwLock<ClientView>>,
}

pub struct WebServer {
    url: String,
    handle: JoinHandle<()>,
}

impl WebServer {
    pub fn start(state: WebServerState) -> Result<Self> {
        sync_snapshot(&state);

        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .context("failed to bind web UI listener")?;
        let port = listener.local_addr()?.port();
        let url = format!("http://127.0.0.1:{port}/");
        listener.set_nonblocking(true)?;

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .build()
                .expect("tokio runtime");
            if let Err(err) = rt.block_on(run_server(state, listener)) {
                eprintln!("web UI server error: {err:#}");
            }
        });

        Ok(Self { url, handle })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn wait(self) {
        let _ = self.handle.join();
    }
}

pub fn empty_client_view() -> ClientView {
    ClientView {
        file: String::new(),
        row_count: 0,
        scan_done: false,
        scan_error: false,
        selected_row: 0,
        selected_col: 0,
        show_column_info: false,
        column_info: None,
        show_help: false,
        show_row_json: false,
        row_json: None,
        row_json_row: None,
        status_line: String::new(),
        column_list_offset: 0,
        column_count: 0,
        table: ClientTable {
            row_start: 0,
            row_end: 0,
            columns: Vec::new(),
            rows: Vec::new(),
        },
        sidebar: Vec::new(),
        table_rows_scroll: ScrollMeta {
            offset: 0,
            total: 0,
            viewport: 0,
        },
        table_cols_scroll: ScrollMeta {
            offset: 0,
            total: 0,
            viewport: 0,
        },
        sidebar_scroll: ScrollMeta {
            offset: 0,
            total: 0,
            viewport: 0,
        },
    }
}

pub fn sync_snapshot(state: &WebServerState) {
    let layout = current_layout(state);
    let view = state.model.lock().unwrap().client_view(layout);
    if let Ok(mut snapshot) = state.snapshot.write() {
        *snapshot = view;
    }
}

async fn run_server(state: WebServerState, listener: std::net::TcpListener) -> Result<()> {
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let app = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/api/action", post(api_action))
        .with_state(state.clone());

    let refresh_state = state.clone();
    tokio::spawn(async move {
        while !refresh_state.model.lock().unwrap().preview.scan_done() {
            sync_snapshot(&refresh_state);
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        sync_snapshot(&refresh_state);
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;

    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_state(State(state): State<WebServerState>) -> Json<ClientView> {
    Json(state.snapshot.read().unwrap().clone())
}

#[derive(Debug, Deserialize)]
struct ActionRequest {
    action: String,
    #[serde(default)]
    value: serde_json::Value,
}

async fn api_action(
    State(state): State<WebServerState>,
    Json(body): Json<ActionRequest>,
) -> impl IntoResponse {
    let action = parse_action(&body.action, &body.value);
    let layout = current_layout(&state);
    let view = {
        let mut model = state.model.lock().unwrap();
        if let Some(action) = action {
            model.apply_action(action, layout);
        }
        model.client_view(layout)
    };
    if let Ok(mut snapshot) = state.snapshot.write() {
        *snapshot = view.clone();
    }
    Json(view)
}

fn current_layout(state: &WebServerState) -> ViewLayout {
    state.layout.lock().map(|l| *l).unwrap_or_default()
}

fn parse_action(name: &str, value: &serde_json::Value) -> Option<ViewAction> {
    match name {
        "row_delta" => value.as_i64().map(|v| ViewAction::RowDelta(v as i32)),
        "col_delta" => value.as_i64().map(|v| ViewAction::ColDelta(v as i32)),
        "column_list_delta" => value.as_i64().map(|v| ViewAction::ColumnListDelta(v as i32)),
        "page_rows" => value.as_i64().map(|v| ViewAction::PageRows(v as i32)),
        "select_column" => value
            .as_u64()
            .or_else(|| value.as_i64().map(|v| v as u64))
            .map(|col| ViewAction::SelectColumn(col as usize)),
        "select_cell" => {
            let row = value.get("row")?.as_u64()? as usize;
            let col = value.get("col")?.as_u64()? as usize;
            Some(ViewAction::SelectCell { row, col })
        }
        "open_column_info" => Some(ViewAction::OpenColumnInfo),
        "close_column_info" => Some(ViewAction::CloseColumnInfo),
        "open_row_json" => Some(ViewAction::OpenRowJson),
        "close_row_json" => Some(ViewAction::CloseRowJson),
        "column_info_focus_delta" => {
            value.as_i64().map(|v| ViewAction::ColumnInfoFocusDelta(v as i32))
        }
        "column_info_apply" => Some(ViewAction::ColumnInfoApply),
        "column_format_focus_delta" => {
            value.as_i64().map(|v| ViewAction::ColumnInfoFocusDelta(v as i32))
        }
        "column_format_apply" => Some(ViewAction::ColumnInfoApply),
        "open_column_format" | "cycle_column_type" => Some(ViewAction::OpenColumnInfo),
        "close_column_format" => Some(ViewAction::CloseColumnInfo),
        "toggle_column_type_labels" | "toggle_types" => Some(ViewAction::OpenColumnInfo),
        "set_column_kind" => {
            let col = value.get("col")?.as_u64()? as usize;
            let kind = column_kind_from_label(value.get("kind")?.as_str()?)?;
            Some(ViewAction::SetColumnKind { col, kind })
        }
        "set_numeric_repr" => {
            let col = value.get("col")?.as_u64()? as usize;
            let repr = numeric_repr_from_label(value.get("repr")?.as_str()?)?;
            Some(ViewAction::SetNumericRepr { col, repr })
        }
        "set_column_decimal_format" => {
            let col = value.get("col")?.as_u64()? as usize;
            let format = value.get("format")?.as_str()?.to_string();
            Some(ViewAction::SetColumnDecimalFormat { col, format })
        }
        "toggle_help" => Some(ViewAction::ToggleHelp),
        "close_help" => Some(ViewAction::CloseHelp),
        "go_home" => Some(ViewAction::GoHome),
        "go_end" => Some(ViewAction::GoEnd),
        "set_column_width" => {
            let col = value.get("col")?.as_u64()? as usize;
            let width = value.get("width")?.as_u64()? as u16;
            Some(ViewAction::SetColumnWidth { col, width })
        }
        "set_row_offset" => value
            .as_u64()
            .or_else(|| value.as_i64().map(|v| v as u64))
            .map(|v| ViewAction::SetRowOffset(v as usize)),
        "set_col_offset" => value
            .as_u64()
            .or_else(|| value.as_i64().map(|v| v as u64))
            .map(|v| ViewAction::SetColOffset(v as usize)),
        "set_column_list_offset" => value
            .as_u64()
            .or_else(|| value.as_i64().map(|v| v as u64))
            .map(|v| ViewAction::SetColumnListOffset(v as usize)),
        "cycle_numeric_repr" => None,
        _ => None,
    }
}
