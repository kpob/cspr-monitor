use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use casper_event_consumer::{EnrichedEvent, EventConsumer};
use tokio::sync::{Mutex, broadcast};

pub mod config;
pub mod filters;
pub mod handler;
pub mod renderer;
pub mod routes;
pub mod state;
pub mod utils;
pub mod widgets;

pub use config::{DashboardConfig, PageConfig, ServiceConfig, ThemeConfig, WidgetConfig, WidgetKind};
pub use state::{ActorStats, AppState, DashboardState, EventRecord};

pub trait EventMapper: Send + Sync + 'static {
    fn map(&self, event: &EnrichedEvent) -> Option<EventRecord>;
}

pub struct Dashboard<M: EventMapper> {
    pub config: DashboardConfig,
    pub mapper: M,
}

impl<M: EventMapper> Dashboard<M> {
    pub fn from_toml(path: impl AsRef<std::path::Path>, mapper: M) -> Result<Self> {
        let config = DashboardConfig::from_toml(path)?;
        Ok(Self { config, mapper })
    }
}

pub async fn run_dashboard<M: EventMapper>(mut dashboard: Dashboard<M>) -> Result<()> {
    dotenv::dotenv().ok();
    casper_common::init_tracing();
    tracing::info!("Starting {}", dashboard.config.service.name);
    casper_common::metrics::install_prometheus_exporter(dashboard.config.service.prometheus_port);

    let static_dir = resolve_static_dir(&mut dashboard.config);
    let config = Arc::new(dashboard.config);

    let (broadcast_tx, _) = broadcast::channel::<EventRecord>(config.service.broadcast_capacity);
    let dashboard_state = Arc::new(Mutex::new(DashboardState::new(config.service.max_events)));

    let app_state = AppState {
        broadcast_tx: broadcast_tx.clone(),
        state: Arc::clone(&dashboard_state),
        service_name: Arc::from(config.service.name.as_str()),
        config: Arc::clone(&config),
    };

    let router = routes::build_router(Arc::clone(&config), app_state, static_dir);

    let web_port = config.service.web_port;
    let service_name = config.service.name.clone();
    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", web_port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                tracing::info!("Web dashboard listening on http://{}", addr);
                if let Err(e) = axum::serve(listener, router).await {
                    tracing::error!("Web server error: {}", e);
                }
            }
            Err(e) => tracing::error!("Failed to bind {}: {}", addr, e),
        }
    });

    let consumer = EventConsumer::builder()
        .brokers(std::env::var("KAFKA_BOOTSTRAP").unwrap_or_else(|_| "localhost:9092".to_string()))
        .topics(config.service.topics.clone())
        .group_id(&config.service.group_id)
        .build()?;

    let mapper = Arc::new(dashboard.mapper);
    let metric_name: Arc<str> = Arc::from(config.service.metric_name.as_str());
    tokio::select! {
        result = consumer.subscribe(handler::DashboardHandler {
            mapper,
            metric_name,
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

fn resolve_static_dir(config: &mut DashboardConfig) -> PathBuf {
    if let Some(dir) = config.static_dir.take() {
        return dir;
    }
    if std::path::Path::new("/app/static").exists() {
        return PathBuf::from("/app/static");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")
}
