use anyhow::Result;
use axum::routing::get;
use axum::{
    Router,
    extract::State,
    response::Json,
};
use axum::response::sse::{Event, KeepAlive, Sse};
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;
use std::{
    collections::{HashMap, VecDeque}, convert::Infallible, marker::PhantomData, sync::Arc
};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;

pub mod utils;
pub mod config;
pub mod state;
pub mod widgets;

pub use state::{ActorStats, AppState, DashboardState, EventRecord};
use state::StatsResponse;

pub trait UiRouter {
    fn ui() -> Router<AppState>;
}

pub trait ApiRouter {
    fn base_api() -> Router<AppState> {
        Router::new()
            .route("/api/stats", get(stats_handler))
    }

    fn api() -> Router<AppState> {
        Self::base_api()
    }
}
// ── Event mapper trait ──────────────────────────────────────────────────────

pub trait EventMapper: Send + Sync + 'static {
    fn map(&self, event: &EnrichedEvent) -> Option<EventRecord>;
}

// ── Configuration ────────────────────────────────────────────────────────────

pub struct DashboardConfig<M: EventMapper, U: UiRouter, A: ApiRouter> {
    pub service_name: &'static str,
    pub web_port: u16,
    pub prometheus_port: u16,
    pub metric_name: &'static str,
    pub topics: Vec<&'static str>,
    pub group_id: &'static str,
    pub mapper: M,
    pub _ui: PhantomData<U>,
    pub _api: PhantomData<A>
}

// ── Kafka event handler ──────────────────────────────────────────────────────

struct DashboardHandler<M: EventMapper> {
    mapper: Arc<M>,
    metric_name: &'static str,
    broadcast_tx: broadcast::Sender<EventRecord>,
    state: Arc<Mutex<DashboardState>>,
}

#[async_trait::async_trait]
impl<M: EventMapper> EventHandler for DashboardHandler<M> {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        let Some(record) = self.mapper.map(&event) else {
            return Ok(());
        };

        metrics::counter!(
            self.metric_name,
            "actor" => record.actor.clone(),
            "action" => record.action.clone()
        )
        .increment(1);

        tracing::info!(
            "[{}] {} {} motes → {} (tx={}, status={})",
            record.actor,
            record.action.to_uppercase(),
            record.amount,
            record.target,
            &record.tx_hash[..record.tx_hash.len().min(12)],
            record.status,
        );

        {
            let mut guard = self.state.lock().await;
            guard.push_event(record.clone());
        }

        let _ = self.broadcast_tx.send(record);
        Ok(())
    }
}

// ── HTTP handlers ────────────────────────────────────────────────────────────
async fn health_handler(State(app): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok", "service": app.service_name}))
}

async fn stats_handler(State(app): State<AppState>) -> Json<StatsResponse> {
    let guard = app.state.lock().await;
    Json(StatsResponse {
        actors: guard.stats.clone(),
        recent_events: guard.events.iter().take(20).cloned().collect(),
    })
}

async fn sse_handler(
    State(app): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = app.broadcast_tx.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| result.ok())
        .map(|record| {
            let data = serde_json::to_string(&record).unwrap_or_default();
            Ok::<Event, Infallible>(Event::default().data(data))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Web server ───────────────────────────────────────────────────────────────
// NOTE: `start_web_server` and `run_dashboard` are commented out for the duration
// of the Task 4 state-split refactor. Task 10 rewrites both wholesale to use the
// new `AppState` shape (with `config: Arc<DashboardConfig>` and
// `service_name: Arc<str>`) and the config-driven max_events.
/*
async fn start_web_server<U: UiRouter, A: ApiRouter>(
    broadcast_tx: broadcast::Sender<EventRecord>,
    state: Arc<Mutex<DashboardState>>,
    service_name: &'static str,
    web_port: u16,
) -> Result<()> {
    let app_state = AppState {
        broadcast_tx,
        state,
        service_name,
    };

    let router = Router::new()
        .route("/health", get(health_handler))
        .route("/events", get(sse_handler))
        .merge(U::ui())
        .merge(A::api())
        .with_state(app_state)
        .nest_service("/static", ServeDir::new("/app/static"));

    let addr = format!("0.0.0.0:{}", web_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Web dashboard listening on http://{}", addr);
    axum::serve(listener, router).await?;
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

pub async fn run_dashboard<M: EventMapper, U: UiRouter, A: ApiRouter>(config: DashboardConfig<M, U, A>) -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();
    tracing::info!("Starting {}", config.service_name);
    casper_common::metrics::install_prometheus_exporter(config.prometheus_port);

    let (broadcast_tx, _) = broadcast::channel::<EventRecord>(100);
    let dashboard_state = Arc::new(Mutex::new(DashboardState::new()));
    {
        let tx = broadcast_tx.clone();
        let state = Arc::clone(&dashboard_state);
        let web_port = config.web_port;

        let service_name = config.service_name;
        tokio::spawn(async move {
            if let Err(e) = start_web_server::<U, A>(tx, state, service_name, web_port).await {
                tracing::error!("Web server error: {}", e);
            }
        });
    }

    let consumer = EventConsumer::builder()
        .brokers(
            std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()),
        )
        .topics(config.topics)
        .group_id(config.group_id)
        .build()?;

    let service_name = config.service_name;
    let mapper = Arc::new(config.mapper);
    tokio::select! {
        result = consumer.subscribe(DashboardHandler {
            mapper,
            metric_name: config.metric_name,
            broadcast_tx,
            state: dashboard_state,
        }) => {
            if let Err(e) = result {
                tracing::error!("Consumer error: {}", e);
            }
        }
        _ = casper_common::shutdown::shutdown_signal() => {
            tracing::info!("Shutting down {}", service_name);
        }
    }

    Ok(())
}
*/
