use std::time::Instant;
use tokio::sync::watch;

pub fn spawn_health_server(port: u16, service: &'static str) -> watch::Sender<Instant> {
    let (tx, rx) = watch::channel(Instant::now());
    tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/health",
            axum::routing::get(move || {
                let rx = rx.clone();
                async move {
                    let age = rx.borrow().elapsed().as_secs();
                    if age > 60 {
                        (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(serde_json::json!({
                                "status": "degraded",
                                "service": service,
                                "last_activity_secs_ago": age
                            })),
                        )
                    } else {
                        (
                            axum::http::StatusCode::OK,
                            axum::Json(serde_json::json!({
                                "status": "ok",
                                "service": service
                            })),
                        )
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
            .await
            .unwrap();
        tracing::info!("Health server listening on :{}", port);
        axum::serve(listener, app).await.unwrap();
    });
    tx
}
