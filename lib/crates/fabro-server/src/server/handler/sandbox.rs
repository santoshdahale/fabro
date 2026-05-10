use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use fabro_sandbox::{TerminalSize, open_terminal_for_run};

use super::super::{
    ApiError, AppState, Bytes, DaytonaSandbox, EnvVars, HeaderMap, IntoResponse, Json,
    NamedTempFile, Path, PreviewUrlRequest, PreviewUrlResponse, Query, RequiredUser, Response,
    Router, RunId, Sandbox, SandboxDetails, SandboxFileEntry, SandboxFileListResponse,
    SandboxProvider, SshAccessRequest, SshAccessResponse, State, StatusCode, collect_causes, fs,
    get, octet_stream_response, parse_run_id_path, post, reconnect_for_run, reject_if_archived,
    render_with_causes, sandbox_details,
};

const MAX_TERMINAL_CONTROL_BYTES: usize = 4096;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/runs/{id}/preview", post(generate_preview_url))
        .route("/runs/{id}/ssh", post(create_ssh_access))
        .route("/runs/{id}/terminal", get(run_terminal))
        .route("/runs/{id}/sandbox", get(retrieve_run_sandbox))
        .route("/runs/{id}/sandbox/files", get(list_sandbox_files))
        .route(
            "/runs/{id}/sandbox/file",
            get(get_sandbox_file).put(put_sandbox_file),
        )
}

async fn retrieve_run_sandbox(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let record = match load_run_sandbox_record_or_not_found(&state, &id).await {
        Ok(record) => record,
        Err(response) => return response,
    };
    let daytona_api_key = state.vault_or_env(EnvVars::DAYTONA_API_KEY);
    let daytona_organization_id = state.vault_or_env(EnvVars::DAYTONA_ORGANIZATION_ID);
    match sandbox_details(&record, daytona_api_key, daytona_organization_id, Some(id)).await {
        Ok(details) => Json::<SandboxDetails>(details).into_response(),
        Err(err) => {
            let detail = format!("{err:#}");
            let status = if detail.contains("has no details implementation") {
                StatusCode::NOT_IMPLEMENTED
            } else {
                StatusCode::CONFLICT
            };
            ApiError::new(status, detail).into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct SandboxFilesParams {
    path:  String,
    #[serde(default)]
    depth: Option<usize>,
}

#[derive(serde::Deserialize)]
struct SandboxFileParams {
    path: String,
}

#[derive(Debug, PartialEq, Eq)]
enum TerminalClientMessage {
    Resize(TerminalSize),
    Close,
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalClientControl {
    Resize { cols: u16, rows: u16 },
    Close,
}

fn parse_terminal_control_message(text: &str) -> Result<TerminalClientMessage, &'static str> {
    if text.len() > MAX_TERMINAL_CONTROL_BYTES {
        return Err("Terminal control message is too large.");
    }
    match serde_json::from_str::<TerminalClientControl>(text) {
        Ok(TerminalClientControl::Resize { cols, rows }) if cols > 0 && rows > 0 => {
            Ok(TerminalClientMessage::Resize(TerminalSize { cols, rows }))
        }
        Ok(TerminalClientControl::Resize { .. }) => {
            Err("Terminal resize dimensions must be greater than zero.")
        }
        Ok(TerminalClientControl::Close) => Ok(TerminalClientMessage::Close),
        Err(_) => Err("Invalid terminal control message."),
    }
}

fn terminal_server_text(message_type: &str, message: Option<&str>) -> WsMessage {
    let payload = match message {
        Some(message) => serde_json::json!({ "type": message_type, "message": message }),
        None => serde_json::json!({ "type": message_type }),
    };
    WsMessage::Text(payload.to_string().into())
}

#[expect(
    clippy::disallowed_types,
    reason = "The Origin header URL is parsed only for same-origin validation and is never logged."
)]
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let Some(host) = headers.get("host").and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Ok(origin_url) = url::Url::parse(origin) else {
        return false;
    };
    let Some(origin_host) = origin_url.host_str() else {
        return false;
    };
    let origin_authority = match origin_url.port_or_known_default() {
        Some(port) => format!("{origin_host}:{port}"),
        None => origin_host.to_string(),
    };
    origin_authority.eq_ignore_ascii_case(host)
}

async fn run_terminal(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    if !origin_allowed(&headers) {
        return ApiError::new(StatusCode::FORBIDDEN, "WebSocket origin is not allowed.")
            .into_response();
    }
    ws.on_upgrade(move |socket| terminal_websocket(socket, state, id))
}

async fn terminal_websocket(mut socket: WebSocket, state: Arc<AppState>, id: RunId) {
    let record = match load_run_sandbox_record(&state, &id).await {
        Ok(record) => record,
        Err(response) => {
            let message = terminal_error_from_status(response.status());
            let _ = socket
                .send(terminal_server_text("error", Some(&message)))
                .await;
            return;
        }
    };
    let daytona_api_key = state.vault_or_env(EnvVars::DAYTONA_API_KEY);
    let daytona_organization_id = state.vault_or_env(EnvVars::DAYTONA_ORGANIZATION_ID);
    let session = match open_terminal_for_run(
        &record,
        daytona_api_key,
        daytona_organization_id,
        Some(id),
        TerminalSize::default(),
    )
    .await
    {
        Ok(session) => session,
        Err(err) => {
            let _ = socket
                .send(terminal_server_text(
                    "error",
                    Some(&err.display_with_causes()),
                ))
                .await;
            return;
        }
    };

    if socket
        .send(terminal_server_text("ready", None))
        .await
        .is_err()
    {
        let _ = session.close().await;
        return;
    }

    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                match message {
                    Ok(WsMessage::Binary(bytes)) => {
                        if let Err(err) = session.write_input(&bytes).await {
                            let _ = socket
                                .send(terminal_server_text("error", Some(&err.display_with_causes())))
                                .await;
                            break;
                        }
                    }
                    Ok(WsMessage::Text(text)) => {
                        match parse_terminal_control_message(text.as_str()) {
                            Ok(TerminalClientMessage::Resize(size)) => {
                                if let Err(err) = session.resize(size).await {
                                    let _ = socket
                                        .send(terminal_server_text("error", Some(&err.display_with_causes())))
                                        .await;
                                    break;
                                }
                            }
                            Ok(TerminalClientMessage::Close) => {
                                let _ = socket.send(terminal_server_text("closed", None)).await;
                                break;
                            }
                            Err(message) => {
                                let _ = socket.send(terminal_server_text("error", Some(message))).await;
                            }
                        }
                    }
                    Ok(WsMessage::Close(_)) => break,
                    Ok(WsMessage::Ping(_) | WsMessage::Pong(_)) => {}
                    Err(err) => {
                        tracing::debug!(error = %err, run_id = %id, "run terminal websocket closed with error");
                        break;
                    }
                }
            }
            output = session.read_output() => {
                match output {
                    Ok(Some(bytes)) => {
                        if socket.send(WsMessage::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = socket.send(terminal_server_text("closed", None)).await;
                        break;
                    }
                    Err(err) => {
                        let _ = socket
                            .send(terminal_server_text("error", Some(&err.display_with_causes())))
                            .await;
                        break;
                    }
                }
            }
        }
    }
    if let Err(err) = session.close().await {
        tracing::warn!(error = %err.display_with_causes(), run_id = %id, "failed to close run terminal session");
    }
}

fn terminal_error_from_status(status: StatusCode) -> String {
    status
        .canonical_reason()
        .unwrap_or("Terminal unavailable")
        .to_string()
}

async fn generate_preview_url(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<PreviewUrlRequest>,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let Ok(port) = u16::try_from(request.port) else {
        return ApiError::bad_request("Port must fit in a u16.").into_response();
    };
    let Ok(expires_in_secs) = i32::try_from(request.expires_in_secs.get()) else {
        return ApiError::bad_request("Preview expiry exceeds supported range.").into_response();
    };

    let sandbox = match reconnect_daytona_sandbox(&state, &id).await {
        Ok(sandbox) => sandbox,
        Err(response) => return response,
    };

    let response = if request.signed {
        match sandbox
            .get_signed_preview_url(port, Some(expires_in_secs))
            .await
        {
            Ok(preview) => PreviewUrlResponse {
                token: None,
                url:   preview.url,
            },
            Err(err) => {
                return ApiError::new(StatusCode::CONFLICT, err.display_with_causes())
                    .into_response();
            }
        }
    } else {
        match sandbox.get_preview_link(port).await {
            Ok(preview) => PreviewUrlResponse {
                token: Some(preview.token),
                url:   preview.url,
            },
            Err(err) => {
                return ApiError::new(StatusCode::CONFLICT, err.display_with_causes())
                    .into_response();
            }
        }
    };

    (StatusCode::CREATED, Json(response)).into_response()
}

async fn create_ssh_access(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<SshAccessRequest>,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let record = match load_run_sandbox_record(&state, &id).await {
        Ok(record) => record,
        Err(response) => return response,
    };

    match record.provider.as_str() {
        provider if provider == SandboxProvider::Daytona.to_string() => {
            let sandbox = match reconnect_daytona_sandbox(&state, &id).await {
                Ok(sandbox) => sandbox,
                Err(response) => return response,
            };
            match sandbox.create_ssh_access(Some(request.ttl_minutes)).await {
                Ok(command) => {
                    (StatusCode::CREATED, Json(SshAccessResponse { command })).into_response()
                }
                Err(err) => {
                    ApiError::new(StatusCode::CONFLICT, err.display_with_causes()).into_response()
                }
            }
        }
        provider if provider == SandboxProvider::Docker.to_string() => {
            let sandbox = match reconnect_run_sandbox(&state, &id).await {
                Ok(sandbox) => sandbox,
                Err(response) => return response,
            };
            match sandbox.ssh_access_command().await {
                Ok(Some(command)) => {
                    (StatusCode::CREATED, Json(SshAccessResponse { command })).into_response()
                }
                Ok(None) => ApiError::new(
                    StatusCode::CONFLICT,
                    "Sandbox provider does not support access commands.",
                )
                .into_response(),
                Err(err) => {
                    ApiError::new(StatusCode::CONFLICT, err.display_with_causes()).into_response()
                }
            }
        }
        _ => ApiError::new(
            StatusCode::CONFLICT,
            "Sandbox provider does not support access commands.",
        )
        .into_response(),
    }
}

async fn list_sandbox_files(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<SandboxFilesParams>,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let sandbox = match reconnect_run_sandbox(&state, &id).await {
        Ok(sandbox) => sandbox,
        Err(response) => return response,
    };
    match sandbox.list_directory(&params.path, params.depth).await {
        Ok(entries) => Json(SandboxFileListResponse {
            data: entries
                .into_iter()
                .map(|entry| SandboxFileEntry {
                    is_dir: entry.is_dir,
                    name:   entry.name,
                    size:   entry.size.map(u64::cast_signed),
                })
                .collect(),
        })
        .into_response(),
        Err(err) => ApiError::new(StatusCode::NOT_FOUND, err.display_with_causes()).into_response(),
    }
}

async fn get_sandbox_file(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<SandboxFileParams>,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let sandbox = match reconnect_run_sandbox(&state, &id).await {
        Ok(sandbox) => sandbox,
        Err(response) => return response,
    };
    let temp = match NamedTempFile::new() {
        Ok(temp) => temp,
        Err(err) => {
            return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
                .into_response();
        }
    };
    if let Err(err) = sandbox
        .download_file_to_local(&params.path, temp.path())
        .await
    {
        return ApiError::new(StatusCode::NOT_FOUND, err.display_with_causes()).into_response();
    }
    match fs::read(temp.path()).await {
        Ok(bytes) => octet_stream_response(bytes.into()),
        Err(err) => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

async fn put_sandbox_file(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<SandboxFileParams>,
    body: Bytes,
) -> Response {
    let id = match parse_run_id_path(&id) {
        Ok(id) => id,
        Err(response) => return response,
    };
    if let Some(response) = reject_if_archived(state.as_ref(), &id).await {
        return response;
    }
    let sandbox = match reconnect_run_sandbox(&state, &id).await {
        Ok(sandbox) => sandbox,
        Err(response) => return response,
    };
    let temp = match NamedTempFile::new() {
        Ok(temp) => temp,
        Err(err) => {
            return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
                .into_response();
        }
    };
    if let Err(err) = fs::write(temp.path(), &body).await {
        return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }
    match sandbox
        .upload_file_from_local(temp.path(), &params.path)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.display_with_causes())
            .into_response(),
    }
}

async fn reconnect_run_sandbox(
    state: &Arc<AppState>,
    run_id: &RunId,
) -> Result<Box<dyn Sandbox>, Response> {
    let record = load_run_sandbox_record(state, run_id).await?;
    let daytona_api_key = state.vault_or_env(EnvVars::DAYTONA_API_KEY);
    let sandbox = reconnect_for_run(&record, daytona_api_key, Some(*run_id))
        .await
        .map_err(|err| {
            let detail = render_with_causes(&err.to_string(), &collect_causes(err.as_ref()));
            ApiError::new(StatusCode::CONFLICT, detail).into_response()
        })?;
    sandbox.start().await.map_err(|err| {
        ApiError::new(StatusCode::CONFLICT, err.display_with_causes()).into_response()
    })?;
    Ok(sandbox)
}

async fn reconnect_daytona_sandbox(
    state: &Arc<AppState>,
    run_id: &RunId,
) -> Result<DaytonaSandbox, Response> {
    let record = load_run_sandbox_record(state, run_id).await?;
    if record.provider != SandboxProvider::Daytona.to_string() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Sandbox provider does not support this capability.",
        )
        .into_response());
    }
    let Some(name) = record.identifier.as_deref() else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Sandbox record is missing the Daytona identifier.",
        )
        .into_response());
    };
    let Some(repo_cloned) = record.repo_cloned else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Sandbox record is missing clone metadata.",
        )
        .into_response());
    };
    let daytona_api_key = state.vault_or_env(EnvVars::DAYTONA_API_KEY);
    let sandbox = DaytonaSandbox::reconnect(
        name,
        daytona_api_key,
        repo_cloned,
        record.clone_origin_url.clone(),
        record.clone_branch.clone(),
    )
    .await
    .map_err(|err| {
        ApiError::new(StatusCode::CONFLICT, err.display_with_causes()).into_response()
    })?;
    sandbox.start().await.map_err(|err| {
        ApiError::new(StatusCode::CONFLICT, err.display_with_causes()).into_response()
    })?;
    Ok(sandbox)
}

async fn load_run_sandbox_record(
    state: &Arc<AppState>,
    run_id: &RunId,
) -> Result<fabro_types::SandboxRecord, Response> {
    match state.store.open_run_reader(run_id).await {
        Ok(run_store) => match run_store.state().await {
            Ok(run_state) => run_state.sandbox.ok_or_else(|| {
                ApiError::new(StatusCode::CONFLICT, "Run has no active sandbox.").into_response()
            }),
            Err(err) => Err(
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
            ),
        },
        Err(_) => Err(ApiError::not_found("Run not found.").into_response()),
    }
}

/// Same as `load_run_sandbox_record`, but treats a missing sandbox record as
/// `404 Not Found` instead of `409 Conflict`. Used by the inspection endpoint
/// where there is no resource to act on if the run never had a sandbox.
async fn load_run_sandbox_record_or_not_found(
    state: &Arc<AppState>,
    run_id: &RunId,
) -> Result<fabro_types::SandboxRecord, Response> {
    match state.store.open_run_reader(run_id).await {
        Ok(run_store) => match run_store.state().await {
            Ok(run_state) => run_state
                .sandbox
                .ok_or_else(|| ApiError::not_found("Run has no sandbox.").into_response()),
            Err(err) => Err(
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
            ),
        },
        Err(_) => Err(ApiError::not_found("Run not found.").into_response()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::*;

    #[test]
    fn terminal_control_accepts_resize_and_close() {
        assert_eq!(
            parse_terminal_control_message(r#"{"type":"resize","cols":120,"rows":32}"#),
            Ok(TerminalClientMessage::Resize(TerminalSize {
                cols: 120,
                rows: 32,
            }))
        );
        assert_eq!(
            parse_terminal_control_message(r#"{"type":"close"}"#),
            Ok(TerminalClientMessage::Close)
        );
    }

    #[test]
    fn terminal_control_rejects_malformed_oversized_and_zero_resize() {
        assert!(parse_terminal_control_message("{").is_err());
        assert!(parse_terminal_control_message(r#"{"type":"resize","cols":0,"rows":32}"#).is_err());
        assert!(
            parse_terminal_control_message(&"x".repeat(MAX_TERMINAL_CONTROL_BYTES + 1)).is_err()
        );
    }

    #[test]
    fn origin_validation_allows_absent_and_same_origin() {
        assert!(origin_allowed(&HeaderMap::new()));

        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:4187"));
        headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:4187"));
        assert!(origin_allowed(&headers));
    }

    #[test]
    fn origin_validation_rejects_cross_origin_browser_origin() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:4187"));
        headers.insert("origin", HeaderValue::from_static("https://evil.example"));
        assert!(!origin_allowed(&headers));
    }
}

#[cfg(test)]
mod retrieve_sandbox_tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use fabro_types::{Graph, RunId, WorkflowSettings};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::test_support::{build_test_router, test_app_state};

    fn req_get(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .expect("sandbox details GET request should build")
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should fit in memory");
        serde_json::from_slice(&bytes).expect("response body should be valid JSON")
    }

    async fn append_run_created(run_store: &fabro_store::RunDatabase, run_id: &RunId) {
        let payload = fabro_store::EventPayload::new(
            json!({
                "id": "evt-run-created",
                "ts": "2026-05-09T11:59:00Z",
                "run_id": run_id,
                "event": "run.created",
                "properties": {
                    "settings": WorkflowSettings::default(),
                    "graph": Graph::new("test"),
                    "run_dir": "/tmp/test",
                },
            }),
            run_id,
        )
        .expect("run.created payload should validate");
        run_store.append_event(&payload).await.unwrap();
    }

    #[tokio::test]
    async fn missing_run_returns_404() {
        let app = build_test_router(test_app_state());
        let absent = RunId::new();
        let response = app
            .oneshot(req_get(&format!("/api/v1/runs/{absent}/sandbox")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_json(response).await;
        assert!(
            body["errors"][0]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("Run not found"),
            "unexpected body: {body}"
        );
    }

    #[tokio::test]
    async fn run_without_sandbox_record_returns_404() {
        let state = test_app_state();
        let app = build_test_router(state.clone());
        let run_id = RunId::new();
        let run_store = state
            .store_ref()
            .create_run(&run_id)
            .await
            .expect("test run should be creatable");
        append_run_created(&run_store, &run_id).await;
        let response = app
            .oneshot(req_get(&format!("/api/v1/runs/{run_id}/sandbox")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_json(response).await;
        assert!(
            body["errors"][0]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("Run has no sandbox"),
            "unexpected body: {body}"
        );
    }

    #[tokio::test]
    async fn local_sandbox_returns_provider_neutral_details() {
        let state = test_app_state();
        let app = build_test_router(state.clone());
        let run_id = RunId::new();
        let run_store = state
            .store_ref()
            .create_run(&run_id)
            .await
            .expect("test run should be creatable");
        append_run_created(&run_store, &run_id).await;
        let payload = fabro_store::EventPayload::new(
            json!({
                "id": "evt-sandbox-init",
                "ts": "2026-05-09T12:00:00Z",
                "run_id": run_id,
                "event": "sandbox.initialized",
                "properties": {
                    "provider": "local",
                    "working_directory": "/workspace",
                },
            }),
            &run_id,
        )
        .expect("sandbox.initialized payload should validate");
        run_store.append_event(&payload).await.unwrap();

        let response = app
            .oneshot(req_get(&format!("/api/v1/runs/{run_id}/sandbox")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["provider"], "local");
        assert_eq!(body["state"], "running");
        assert!(body["resources"].is_object());
        assert!(body["timestamps"].is_object());
    }

    #[tokio::test]
    async fn unknown_provider_returns_501() {
        let state = test_app_state();
        let app = build_test_router(state.clone());
        let run_id = RunId::new();
        let run_store = state
            .store_ref()
            .create_run(&run_id)
            .await
            .expect("test run should be creatable");
        append_run_created(&run_store, &run_id).await;
        let payload = fabro_store::EventPayload::new(
            json!({
                "id": "evt-sandbox-init",
                "ts": "2026-05-09T12:00:00Z",
                "run_id": run_id,
                "event": "sandbox.initialized",
                "properties": {
                    "provider": "ephemeral-mystery-cloud",
                    "working_directory": "/workspace",
                },
            }),
            &run_id,
        )
        .expect("sandbox.initialized payload should validate");
        run_store.append_event(&payload).await.unwrap();

        let response = app
            .oneshot(req_get(&format!("/api/v1/runs/{run_id}/sandbox")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }
}
