use anyhow::Result;
use axum::{
    Router,
    extract::State,
    response::{Html, Json},
    routing::get,
};
use axum::response::sse::{Event, KeepAlive, Sse};
use casper_event_consumer::{EnrichedEvent, EventConsumer, EventHandler};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    sync::Arc,
};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;

// ── Shared state types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventRecord {
    timestamp: String,
    exchange: String,
    exchange_address: String,
    direction: String,
    amount: u64,
    counterparty: String,
    tx_hash: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ExchangeStats {
    inflow_total: u64,
    outflow_total: u64,
    tx_count: u64,
}

struct DashboardState {
    events: VecDeque<EventRecord>,
    stats: HashMap<String, ExchangeStats>,
}

impl DashboardState {
    fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(50),
            stats: HashMap::new(),
        }
    }

    fn push_event(&mut self, record: EventRecord) {
        let entry = self.stats.entry(record.exchange.clone()).or_default();
        entry.tx_count += 1;
        if record.direction == "inflow" {
            entry.inflow_total += record.amount;
        } else {
            entry.outflow_total += record.amount;
        }
        self.events.push_front(record);
        self.events.truncate(50);
    }
}

// ── Axum app state ───────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    broadcast_tx: broadcast::Sender<EventRecord>,
    state: Arc<Mutex<DashboardState>>,
}

// ── Kafka event handler ──────────────────────────────────────────────────────

struct ExchangeHandler {
    filter_exchange: Option<String>,
    broadcast_tx: broadcast::Sender<EventRecord>,
    state: Arc<Mutex<DashboardState>>,
}

#[async_trait::async_trait]
impl EventHandler for ExchangeHandler {
    async fn handle(&self, event: EnrichedEvent) -> Result<()> {
        let exchange = event.app_data["exchange"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string();

        if let Some(filter) = &self.filter_exchange {
            if exchange != *filter {
                return Ok(());
            }
        }

        let direction = event.app_data["direction"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let counterparty = event.app_data["counterparty"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let amount: u64 = event.app_data["amount"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let preposition = if direction == "inflow" { "from" } else { "to" };
        tracing::info!(
            "[{}] {} {} motes {} {} (tx={}, status={})",
            exchange,
            direction.to_uppercase(),
            amount,
            preposition,
            counterparty,
            &event.tx_hash[..event.tx_hash.len().min(12)],
            event.lifecycle.status,
        );

        let record = EventRecord {
            timestamp: event.lifecycle.processed_at.clone(),
            exchange,
            exchange_address: event.lifecycle.sender.clone(),
            direction,
            amount,
            counterparty,
            tx_hash: event.tx_hash.clone(),
            status: event.lifecycle.status.clone(),
        };

        {
            let mut guard = self.state.lock().await;
            guard.push_event(record.clone());
        }

        let _ = self.broadcast_tx.send(record);

        Ok(())
    }
}

// ── HTTP handlers ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatsResponse {
    exchanges: HashMap<String, ExchangeStats>,
    recent_events: Vec<EventRecord>,
}

async fn index_handler() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn stats_handler(State(app): State<AppState>) -> Json<StatsResponse> {
    let guard = app.state.lock().await;
    Json(StatsResponse {
        exchanges: guard.stats.clone(),
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

async fn start_web_server(
    broadcast_tx: broadcast::Sender<EventRecord>,
    state: Arc<Mutex<DashboardState>>,
) -> Result<()> {
    let app_state = AppState { broadcast_tx, state };
    let router = Router::new()
        .route("/", get(index_handler))
        .route("/api/stats", get(stats_handler))
        .route("/events", get(sse_handler))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Web dashboard listening on http://0.0.0.0:8080");
    axum::serve(listener, router).await?;
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();

    tracing::info!("Starting casper-exchange-monitor");

    let filter_exchange = std::env::var("EXCHANGE_FILTER").ok();
    match &filter_exchange {
        Some(name) => tracing::info!("Filtering for exchange: {}", name),
        None => tracing::info!("Monitoring all exchanges (set EXCHANGE_FILTER=<name> to narrow)"),
    }

    let (broadcast_tx, _) = broadcast::channel::<EventRecord>(100);
    let dashboard_state = Arc::new(Mutex::new(DashboardState::new()));

    {
        let tx = broadcast_tx.clone();
        let state = Arc::clone(&dashboard_state);
        tokio::spawn(async move {
            if let Err(e) = start_web_server(tx, state).await {
                tracing::error!("Web server error: {}", e);
            }
        });
    }

    let consumer = EventConsumer::builder()
        .brokers(
            std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()),
        )
        .topics(vec!["apps.exchanges"])
        .group_id("exchange-monitor-v1")
        .build()?;

    consumer
        .subscribe(ExchangeHandler {
            filter_exchange,
            broadcast_tx,
            state: dashboard_state,
        })
        .await?;

    Ok(())
}

// ── Embedded dashboard HTML ───────────────────────────────────────────────────

const DASHBOARD_HTML: &str = include_str!("dashboard.html");

