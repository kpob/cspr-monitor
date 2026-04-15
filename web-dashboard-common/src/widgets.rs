use crate::config::{PageConfig, WidgetConfig, WidgetKind};

/// Row and column span used by the CSS grid.
pub struct GridCell {
    pub row: u32,
    pub col: u32,
    pub width: u32,
}

impl From<&WidgetConfig> for GridCell {
    fn from(w: &WidgetConfig) -> Self {
        Self { row: w.row, col: w.col, width: w.width }
    }
}

/// Total number of grid columns for a page — max(col + width - 1) across widgets.
pub fn grid_columns(page: &PageConfig) -> u32 {
    page.widgets
        .iter()
        .map(|w| w.col + w.width.saturating_sub(1))
        .max()
        .unwrap_or(1)
        .max(1)
}

/// Rendered string used as `type` attribute in data-widget containers.
pub fn kind_str(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::StatsCards => "stats_cards",
        WidgetKind::BarChart => "bar_chart",
        WidgetKind::PieChart => "pie_chart",
        WidgetKind::EventTable => "event_table",
        WidgetKind::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PageConfig, WidgetConfig, WidgetKind};

    fn w(row: u32, col: u32, width: u32) -> WidgetConfig {
        WidgetConfig {
            kind: WidgetKind::BarChart,
            id: format!("w{}{}", row, col),
            row,
            col,
            width,
            title: None,
            group_by: None,
            metrics: vec![],
            datasets: vec![],
            columns: vec![],
            max_rows: None,
            widget_key: None,
            js: None,
            css: None,
        }
    }

    #[test]
    fn grid_columns_picks_max_extent() {
        let page = PageConfig {
            path: "/".into(),
            title: "t".into(),
            subtitle: None,
            filter_field: None,
            widgets: vec![w(1, 1, 1), w(1, 2, 1), w(2, 1, 2)],
        };
        assert_eq!(grid_columns(&page), 2);
    }
}
