use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpListener;

use crate::cli::observability::{AgentState, render_metrics};

pub(super) fn router(state: Arc<AgentState>) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
        .with_state(state)
}

pub(super) async fn serve(
    listener: TcpListener,
    state: Arc<AgentState>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await
}

async fn health() -> &'static str {
    "ok\n"
}

async fn ready(State(state): State<Arc<AgentState>>) -> impl IntoResponse {
    if state.is_ready() {
        (StatusCode::OK, "ready\n")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready\n")
    }
}

async fn metrics(State(state): State<Arc<AgentState>>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        render_metrics(&state),
    )
}

#[cfg(test)]
mod tests {
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::models::{DiagnosisContext, DiagnosisResult, TargetMetadata};
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use chrono::Utc;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn endpoints_expose_health_readiness_and_metrics() {
        let state = Arc::new(AgentState::new(TargetMetadata::default()));
        let app = router(Arc::clone(&state));

        let health = app
            .clone()
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(health.into_body(), usize::MAX).await.unwrap(),
            "ok\n"
        );

        let ready = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);

        let result = DiagnosisResult::new(
            DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            vec![],
        );
        state.record_success(&result, Utc::now());
        let ready = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::OK);

        state.record_error();
        let ready = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);

        state.record_success(&result, Utc::now());
        let ready = app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::OK);

        let metrics = app
            .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(metrics.status(), StatusCode::OK);
        assert_eq!(
            metrics.headers()[header::CONTENT_TYPE],
            "text/plain; version=0.0.4; charset=utf-8"
        );
        let body = to_bytes(metrics.into_body(), usize::MAX).await.unwrap();
        assert!(body.starts_with(b"# HELP vllm_doctor_ready "));
    }

    #[tokio::test]
    async fn server_stops_cleanly_on_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = Arc::new(AgentState::new(TargetMetadata::default()));
        let (send, receive) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve(listener, state, async move {
            let _ = receive.await;
        }));
        send.send(()).unwrap();
        server.await.unwrap().unwrap();
    }
}
