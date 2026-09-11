//! 设备码登录的 HTTP 绑定；业务状态由 cli_session 事务管理。
use super::AppState;
use crate::{
    auth::cli_session::{
        self, CliSessionApproved, CliSessionPoll, ConfirmCliSessionRequest,
        CreateCliSessionResponse,
    },
    middleware::session_auth::SessionAuth,
};
use axfetchum::ApiRouter;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(untagged)]
pub enum CliPollResponse {
    Approved(CliSessionApproved),
    Status(CliSessionPoll),
}

pub fn routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .group("cliAuth")
        .post("/api/v1/auth/cli-sessions", create_session)
        .response::<CreateCliSessionResponse>()
        .done()
        .get("/api/v1/auth/cli-sessions/{sessionId}", poll_session)
        .response::<CliPollResponse>()
        .done()
        .post("/api/v1/auth/cli-sessions/confirm", confirm_session)
        .json::<ConfirmCliSessionRequest, CliSessionPoll>()
        .auth()
        .done()
        .get("/auth/cli-verify", verify_page)
        .redirect()
        .done()
}

fn json_response(status: StatusCode, body: impl Serialize) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

fn failure(status: StatusCode, message: &str) -> Response {
    json_response(status, serde_json::json!({"error": message}))
}

fn internal_error(error: String) -> Response {
    tracing::error!(%error, "device login operation failed");
    failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "device login operation failed",
    )
}

async fn create_session(State(state): State<AppState>) -> Result<Response, Response> {
    let created = cli_session::create_session(&state.db, &state.public_base_url)
        .await
        .map_err(internal_error)?;
    Ok(json_response(StatusCode::OK, created))
}

async fn poll_session(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Response, Response> {
    let result = cli_session::poll_and_consume(&state.db, &key)
        .await
        .map_err(internal_error)?;
    Ok(match result {
        None => failure(StatusCode::NOT_FOUND, "session not found"),
        Some(Ok(approved)) => json_response(StatusCode::OK, approved),
        Some(Err(status)) => {
            let pending = status.status == "pending";
            let mut response = json_response(
                if pending {
                    StatusCode::ACCEPTED
                } else {
                    StatusCode::GONE
                },
                status,
            );
            if pending {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, "5".parse().unwrap());
            }
            response
        }
    })
}

async fn confirm_session(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Json(request): Json<ConfirmCliSessionRequest>,
) -> Result<Response, Response> {
    match cli_session::confirm_session(&state.db, user.user_id, &request).await {
        Ok(Some(())) => Ok(json_response(
            StatusCode::OK,
            CliSessionPoll {
                status: "approved".into(),
            },
        )),
        Ok(None) => Err(failure(StatusCode::NOT_FOUND, "user code not found")),
        Err(error) => Err(match error.as_str() {
            "session expired" => failure(StatusCode::GONE, &error),
            "session already confirmed or consumed" => failure(StatusCode::CONFLICT, &error),
            "account disabled" => failure(StatusCode::UNAUTHORIZED, &error),
            _ => internal_error(error),
        }),
    }
}

#[derive(Deserialize)]
struct VerifyQuery {
    code: Option<String>,
}

async fn verify_page(
    user: Result<SessionAuth, Response>,
    Query(query): Query<VerifyQuery>,
) -> Result<Redirect, Response> {
    let suffix = match query.code {
        Some(code) if code.len() == 6 && code.bytes().all(|byte| (b'2'..=b'9').contains(&byte)) => {
            format!("?code={code}")
        }
        Some(_) => return Err(failure(StatusCode::BAD_REQUEST, "invalid user code")),
        None => String::new(),
    };
    if user.is_err() {
        let mut login = reqwest::Url::parse("http://localhost/auth/login").unwrap();
        login
            .query_pairs_mut()
            .append_pair("next", &format!("/auth/cli-verify{suffix}"));
        return Ok(Redirect::temporary(&format!(
            "/auth/login?{}",
            login.query().unwrap()
        )));
    }
    Ok(Redirect::temporary(&format!("/#/auth/cli-verify{suffix}")))
}
