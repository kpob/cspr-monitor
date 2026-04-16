use web_dashboard_common::config::DashboardConfig;
use web_dashboard_common::renderer::render_page;

const TOML: &str = r#"
[service]
name = "test"
web_port = 8080
prometheus_port = 9000
metric_name = "t"
topics = ["t"]
group_id = "g"

[[pages]]
path = "/"
title = "Test Dashboard"
subtitle = "Smoke"

[[pages.widgets]]
type = "bar_chart"
id = "vol"
row = 1
col = 1
title = "Volume"

[[pages.widgets]]
type = "event_table"
id = "log"
row = 2
col = 1
width = 2
columns = ["timestamp", "actor", "action"]
"#;

#[test]
fn renders_configured_widgets() {
    let cfg = DashboardConfig::from_toml_str(TOML).unwrap();
    let page = &cfg.pages[0];
    let html = render_page(&cfg, page, vec![], vec![], "/api/stats".into(), "/events".into()).unwrap();
    assert!(html.contains("data-widget=\"bar_chart\""));
    assert!(html.contains("data-widget-id=\"vol\""));
    assert!(html.contains("data-widget=\"event_table\""));
    assert!(html.contains("Test Dashboard"));
    assert!(html.contains("grid-template-columns: repeat(2, 1fr)"));
}
