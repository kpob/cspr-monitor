use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State},
    response::{Html, Json, sse::{Event, KeepAlive, Sse}},
    routing::get,
};
use futures_util::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::ServeDir;

use crate::config::{DashboardConfig, extract_param};
use crate::filters::{filter_by_address, filter_by_field};
use crate::renderer::render_page;
use crate::state::{AppState, EventRecord, StatsResponse};

pub fn build_router(
    cfg: Arc<DashboardConfig>,
    state: AppState,
    static_dir: PathBuf,
) -> Router {
    let mut router = Router::new()
        .route("/health", get(health_handler))
        .route("/events", get(sse_handler))
        .route("/api/stats", get(stats_handler))
        .route("/api/config", get(config_handler));

    let custom_css: Vec<String> = collect_custom_assets(&cfg, |w| w.css.as_deref());
    let custom_js: Vec<String> = collect_custom_assets(&cfg, |w| w.js.as_deref());

    for page in &cfg.pages {
        let param = extract_param(&page.path);
        let path_axum = to_axum_path(&page.path);

        let rendered = render_page(
            &cfg,
            page,
            custom_css.clone(),
            custom_js.clone(),
            "/api/stats".into(),
            "/events".into(),
        )
        .expect("template render");
        let html = Arc::<str>::from(rendered);

        router = router.route(
            &path_axum,
            get({
                let html = html.clone();
                move || async move { Html(html.to_string()) }
            }),
        );

        match param.as_deref() {
            Some("address") => {
                let api = format!("/api{}", path_axum);
                router = router.route(&api, get(address_api_handler));
            }
            Some(_) => {
                let api = format!("/api{}", path_axum);
                let filter_field = page
                    .filter_field
                    .clone()
                    .expect("validated at startup");
                router = router.route(
                    &api,
                    get({
                        let filter_field = Arc::<str>::from(filter_field);
                        move |State(app): State<AppState>, Path(value): Path<String>| {
                            let f = filter_field.clone();
                            async move {
                                let guard = app.state.lock().await;
                                Json(filter_by_field(&guard, &f, &value))
                            }
                        }
                    }),
                );
            }
            None => {}
        }
    }

    router
        .with_state(state)
        .nest_service("/static", ServeDir::new(static_dir))
}

fn to_axum_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    let mut chars = p.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            out.push(':');
            for c2 in chars.by_ref() {
                if c2 == '}' { break; }
                out.push(c2);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn collect_custom_assets<F>(cfg: &DashboardConfig, mut pick: F) -> Vec<String>
where
    F: FnMut(&crate::config::WidgetConfig) -> Option<&str>,
{
    let mut seen = std::collections::BTreeSet::new();
    for page in &cfg.pages {
        for w in &page.widgets {
            if let Some(v) = pick(w) { seen.insert(v.to_string()); }
        }
    }
    seen.into_iter().collect()
}

async fn health_handler(State(app): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": app.service_name.as_ref()}))
}

async fn stats_handler(State(app): State<AppState>) -> Json<StatsResponse> {
    let guard = app.state.lock().await;
    Json(StatsResponse {
        actors: guard.stats.clone(),
        recent_events: guard.events.iter().take(20).cloned().collect(),
    })
}

async fn config_handler(State(app): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::to_value(app.config.as_ref()).unwrap_or_else(|_| serde_json::json!({})))
}

async fn address_api_handler(
    State(app): State<AppState>,
    Path(address): Path<String>,
) -> Json<crate::filters::AddressEventsResponse> {
    let guard = app.state.lock().await;
    Json(filter_by_address(&guard, &address))
}

async fn sse_handler(
    State(app): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = app.broadcast_tx.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|result: Result<EventRecord, _>| result.ok())
        .map(|record| {
            let data = serde_json::to_string(&record).unwrap_or_default();
            Ok::<Event, Infallible>(Event::default().data(data))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
