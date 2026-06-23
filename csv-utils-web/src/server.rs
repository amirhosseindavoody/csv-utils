use crate::assets::INDEX_HTML;
use anyhow::Result;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use csv_utils_core::{AppModel, ClientView, ViewAction, ViewLayout};
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

const LAYOUT: ViewLayout = ViewLayout {
    viewport_rows: 24,
    table_width: 110,
    column_list_height: 20,
};

type SharedModel = Arc<Mutex<AppModel>>;

pub async fn run(file: Option<PathBuf>, host: &str, port: u16) -> Result<()> {
    let model = AppModel::open(file)?;
    let state: SharedModel = Arc::new(Mutex::new(model));

    let app = Router::new()
        .route("/", get(index))
        .route("/api/state", get(api_state))
        .route("/api/action", post(api_action))
        .with_state(state.clone());

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    eprintln!("csv-utils web UI at http://{host}:{port}/");
    if host == "127.0.0.1" || host == "localhost" {
        eprintln!("(use --host 0.0.0.0 to listen on all interfaces)");
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    state.lock().await.join_scan_thread();
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_state(State(state): State<SharedModel>) -> Json<ClientView> {
    let model = state.lock().await;
    Json(model.client_view(LAYOUT))
}

#[derive(Debug, Deserialize)]
struct ActionRequest {
    action: String,
    #[serde(default)]
    value: serde_json::Value,
}

async fn api_action(
    State(state): State<SharedModel>,
    Json(body): Json<ActionRequest>,
) -> impl IntoResponse {
    let action = parse_action(&body.action, &body.value);
    let mut model = state.lock().await;
    if let Some(action) = action {
        model.apply_action(action, LAYOUT);
    }
    Json(model.client_view(LAYOUT))
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
        "toggle_types" => Some(ViewAction::ToggleTypes),
        "toggle_help" => Some(ViewAction::ToggleHelp),
        "close_help" => Some(ViewAction::CloseHelp),
        "go_home" => Some(ViewAction::GoHome),
        "go_end" => Some(ViewAction::GoEnd),
        "set_column_width" => {
            let col = value.get("col")?.as_u64()? as usize;
            let width = value.get("width")?.as_u64()? as u16;
            Some(ViewAction::SetColumnWidth { col, width })
        }
        "cycle_column_type" => Some(ViewAction::CycleColumnType),
        "cycle_numeric_repr" => Some(ViewAction::CycleNumericRepr),
        _ => None,
    }
}
