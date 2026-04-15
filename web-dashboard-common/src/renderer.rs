use askama::Template;
use serde::Serialize;

use crate::config::{DashboardConfig, PageConfig, ThemeConfig, WidgetConfig, WidgetKind};
use crate::widgets::{grid_columns, kind_str};

#[derive(Template)]
#[template(path = "page.html", escape = "html")]
pub struct PageTemplate<'a> {
    pub page: PageView<'a>,
    pub theme_vars: String,
    pub columns: u32,
    pub bootstrap_url: String,
    pub events_url: String,
    pub custom_css: Vec<String>,
    pub custom_js: Vec<String>,
}

pub struct PageView<'a> {
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub widgets: Vec<WidgetView<'a>>,
}

pub struct WidgetView<'a> {
    pub id: &'a str,
    pub kind: WidgetKind,
    pub kind_str: &'static str,
    pub row: u32,
    pub col: u32,
    pub width: u32,
    pub title: Option<&'a str>,
    pub config_json: String,
}

pub fn theme_css(theme: &ThemeConfig) -> String {
    let mut out = String::new();
    if let Some(a) = &theme.accent {
        out.push_str(&format!("--accent: {};", a));
    }
    for (k, v) in &theme.colors {
        out.push_str(&format!("--color-{}: {};", k, v));
    }
    out
}

pub fn render_page(
    cfg: &DashboardConfig,
    page: &PageConfig,
    custom_css: Vec<String>,
    custom_js: Vec<String>,
    bootstrap_url: String,
    events_url: String,
) -> anyhow::Result<String> {
    let tpl = PageTemplate {
        page: PageView {
            title: &page.title,
            subtitle: page.subtitle.as_deref(),
            widgets: page.widgets.iter().map(widget_view).collect(),
        },
        theme_vars: theme_css(&cfg.theme),
        columns: grid_columns(page),
        bootstrap_url,
        events_url,
        custom_css,
        custom_js,
    };
    Ok(tpl.render()?)
}

fn widget_view(w: &WidgetConfig) -> WidgetView<'_> {
    #[derive(Serialize)]
    struct Cfg<'a> {
        id: &'a str,
        #[serde(rename = "type")]
        kind: &'static str,
        title: Option<&'a str>,
        group_by: Option<&'a str>,
        metrics: &'a [String],
        datasets: &'a [crate::config::DatasetConfig],
        columns: &'a [String],
        max_rows: Option<usize>,
        widget_key: Option<&'a str>,
    }
    let c = Cfg {
        id: &w.id,
        kind: kind_str(w.kind),
        title: w.title.as_deref(),
        group_by: w.group_by.as_deref(),
        metrics: &w.metrics,
        datasets: &w.datasets,
        columns: &w.columns,
        max_rows: w.max_rows,
        widget_key: w.widget_key.as_deref(),
    };
    WidgetView {
        id: &w.id,
        kind: w.kind,
        kind_str: kind_str(w.kind),
        row: w.row,
        col: w.col,
        width: w.width,
        title: w.title.as_deref(),
        config_json: serde_json::to_string(&c).unwrap_or_else(|_| "{}".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PageConfig, ServiceConfig};

    fn cfg(pages: Vec<PageConfig>) -> DashboardConfig {
        DashboardConfig {
            service: ServiceConfig {
                name: "x".into(),
                web_port: 8080,
                prometheus_port: 9000,
                metric_name: "m".into(),
                topics: vec!["t".into()],
                group_id: "g".into(),
                max_events: 100,
                broadcast_capacity: 100,
            },
            theme: ThemeConfig::default(),
            pages,
            static_dir: None,
        }
    }

    #[test]
    fn renders_empty_page_with_title() {
        let page = PageConfig {
            path: "/".into(), title: "Hello".into(), subtitle: None,
            filter_field: None, widgets: vec![],
        };
        let c = cfg(vec![page.clone()]);
        let html = render_page(&c, &page, vec![], vec![], "/api/stats".into(), "/events".into()).unwrap();
        assert!(html.contains("<title>Hello</title>"));
        assert!(html.contains("class=\"grid\""));
    }
}
