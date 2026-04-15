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

impl DashboardConfig {
    pub fn from_toml_str(src: &str) -> anyhow::Result<Self> {
        let cfg: DashboardConfig = toml::from_str(src)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_toml(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let src = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {}", path.display(), e))?;
        Self::from_toml_str(&src)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.pages.is_empty() {
            anyhow::bail!("dashboard config error: pages: at least one page required");
        }
        let mut seen_ids = std::collections::HashSet::new();
        for (pi, page) in self.pages.iter().enumerate() {
            if page.path.is_empty() {
                anyhow::bail!("dashboard config error: pages[{}].path: required", pi);
            }
            if page.title.is_empty() {
                anyhow::bail!("dashboard config error: pages[{}].title: required", pi);
            }

            for (wi, w) in page.widgets.iter().enumerate() {
                let where_ = format!("pages[{}].widgets[{}]", pi, wi);
                if w.id.is_empty() {
                    anyhow::bail!("dashboard config error: {}.id: required", where_);
                }
                if !seen_ids.insert((pi, w.id.clone())) {
                    anyhow::bail!("dashboard config error: {}.id: duplicate id '{}'", where_, w.id);
                }
                if w.row < 1 || w.col < 1 {
                    anyhow::bail!("dashboard config error: {}: row/col must be >= 1", where_);
                }
                if w.kind == WidgetKind::Custom {
                    let key = w.widget_key.as_deref().unwrap_or("");
                    if key.is_empty() {
                        anyhow::bail!(
                            "dashboard config error: {}.widget_key: required for custom widgets",
                            where_
                        );
                    }
                    if !is_valid_js_ident(key) {
                        anyhow::bail!(
                            "dashboard config error: {}.widget_key: '{}' is not a valid JS identifier",
                            where_, key
                        );
                    }
                    if w.js.is_none() {
                        anyhow::bail!(
                            "dashboard config error: {}.js: required for custom widgets",
                            where_
                        );
                    }
                }
            }

            if let Some(param) = extract_param(&page.path) {
                if param != "address" && page.filter_field.is_none() {
                    anyhow::bail!(
                        "dashboard config error: pages[{}]: path has parameter '{{{}}}' that is not 'address' — filter_field required",
                        pi, param
                    );
                }
            }
        }
        Ok(())
    }
}

fn is_valid_js_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

pub fn extract_param(path: &str) -> Option<String> {
    let start = path.find('{')?;
    let end = path.find('}')?;
    if end > start + 1 {
        Some(path[start + 1..end].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_SERVICE: &str = r#"
[service]
name = "x"
web_port = 8080
prometheus_port = 9000
metric_name = "x_total"
topics = ["t"]
group_id = "g"
"#;

    fn with_page(extra: &str) -> String {
        format!("{}{}", MIN_SERVICE, extra)
    }

    #[test]
    fn parses_minimal_config() {
        let src = with_page(
            r#"
[[pages]]
path = "/"
title = "Home"
"#,
        );
        let cfg = DashboardConfig::from_toml_str(&src).unwrap();
        assert_eq!(cfg.service.name, "x");
        assert_eq!(cfg.service.max_events, 100);
        assert_eq!(cfg.pages.len(), 1);
    }

    #[test]
    fn rejects_missing_pages() {
        let err = DashboardConfig::from_toml_str(MIN_SERVICE).unwrap_err();
        assert!(err.to_string().contains("at least one page"));
    }

    #[test]
    fn rejects_duplicate_widget_ids() {
        let src = with_page(
            r#"
[[pages]]
path = "/"
title = "Home"

[[pages.widgets]]
type = "bar_chart"
id = "dup"
row = 1
col = 1

[[pages.widgets]]
type = "event_table"
id = "dup"
row = 2
col = 1
"#,
        );
        let err = DashboardConfig::from_toml_str(&src).unwrap_err();
        assert!(err.to_string().contains("duplicate id"), "got: {}", err);
    }

    #[test]
    fn rejects_custom_without_key() {
        let src = with_page(
            r#"
[[pages]]
path = "/"
title = "Home"

[[pages.widgets]]
type = "custom"
id = "c"
row = 1
col = 1
js = "x.js"
"#,
        );
        let err = DashboardConfig::from_toml_str(&src).unwrap_err();
        assert!(err.to_string().contains("widget_key"), "got: {}", err);
    }

    #[test]
    fn rejects_invalid_widget_key() {
        let src = with_page(
            r#"
[[pages]]
path = "/"
title = "Home"

[[pages.widgets]]
type = "custom"
id = "c"
row = 1
col = 1
widget_key = "9bad"
js = "x.js"
"#,
        );
        let err = DashboardConfig::from_toml_str(&src).unwrap_err();
        assert!(err.to_string().contains("not a valid JS identifier"), "got: {}", err);
    }

    #[test]
    fn accepts_address_param_without_filter_field() {
        let src = with_page(
            r#"
[[pages]]
path = "/account/{address}"
title = "Account"
"#,
        );
        DashboardConfig::from_toml_str(&src).unwrap();
    }

    #[test]
    fn rejects_nonaddress_param_without_filter_field() {
        let src = with_page(
            r#"
[[pages]]
path = "/validator/{hash}"
title = "Validator"
"#,
        );
        let err = DashboardConfig::from_toml_str(&src).unwrap_err();
        assert!(err.to_string().contains("filter_field required"), "got: {}", err);
    }

    #[test]
    fn accepts_nonaddress_param_with_filter_field() {
        let src = with_page(
            r#"
[[pages]]
path = "/validator/{hash}"
title = "Validator"
filter_field = "target"
"#,
        );
        DashboardConfig::from_toml_str(&src).unwrap();
    }
}
