use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardConfig {
    pub service: ServiceConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub pages: Vec<PageConfig>,

    /// Overridable at runtime; not part of the TOML schema.
    #[serde(skip)]
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    pub name: String,
    pub web_port: u16,
    pub prometheus_port: u16,
    pub metric_name: String,
    pub topics: Vec<String>,
    pub group_id: String,
    #[serde(default = "default_max_events")]
    pub max_events: usize,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
}

fn default_max_events() -> usize { 100 }
fn default_broadcast_capacity() -> usize { 100 }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ThemeConfig {
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub colors: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PageConfig {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Used for parameterized pages whose param is not `{address}`.
    #[serde(default)]
    pub filter_field: Option<String>,
    #[serde(default)]
    pub widgets: Vec<WidgetConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetConfig {
    #[serde(rename = "type")]
    pub kind: WidgetKind,
    pub id: String,
    pub row: u32,
    pub col: u32,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub title: Option<String>,

    // stats_cards
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub metrics: Vec<String>,

    // bar_chart / pie_chart
    #[serde(default)]
    pub datasets: Vec<DatasetConfig>,

    // event_table
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub max_rows: Option<usize>,

    // custom
    #[serde(default)]
    pub widget_key: Option<String>,
    #[serde(default)]
    pub js: Option<String>,
    #[serde(default)]
    pub css: Option<String>,
}

fn default_width() -> u32 { 1 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatasetConfig {
    pub field: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub group_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    StatsCards,
    BarChart,
    PieChart,
    EventTable,
    Custom,
}
