# Dashboard Framework Design

Config-driven, composable dashboard framework for the Casper monitor pipeline.

## Problem

The current dashboard system (exchange monitor + whale activity) suffers from:

- **CSS fragmentation** — exchange has minimal 89-line stylesheet, whale has a modern design system with CSS variables. No shared visual identity.
- **HTML/JS duplication** — SSE reconnection, time formatting, address shortening, Chart.js boilerplate, bootstrap patterns are copy-pasted across dashboards.
- **No template structure** — each HTML file is a 200-380 line monolith with inline scripts and styles.
- **Inconsistent file layout** — exchange puts HTML in `src/`, whale puts it in `html/` with separate `static/css/`.
- **High effort to create new dashboards** — requires implementing `UiRouter`, `ApiRouter` traits, writing custom HTML/CSS/JS from scratch.

## Design Goals

1. Creating a new dashboard requires only a Rust mapper + a TOML config file — no HTML/CSS/JS for standard dashboards.
2. Dashboards share a unified design system with per-dashboard theme overrides.
3. Composable widget grid with an escape hatch for custom JS widgets.
4. Multi-page support (e.g., main dashboard + account detail) driven from config.
5. One binary per dashboard — deployed independently.

## Architecture: Hybrid Server-Side + Client-Side Rendering

Rust reads the TOML config at startup and generates HTML pages via Askama templates. Each page contains a CSS grid of empty widget containers with `data-` attributes. A shared JavaScript library (`dashboard.js`) discovers these containers and renders the appropriate Chart.js chart, table, or card widget inside each one.

```
TOML config
    │ (parsed at startup by Rust)
    ▼
Askama templates → HTML pages with widget containers
    │ (served by Axum)
    ▼
Browser loads page
    │
    ▼
dashboard.js discovers [data-widget] containers
    │ → init(el, config)      — create empty chart/table structure
    │ → bootstrap(el, stats)  — populate from /api/stats
    │ → update(el, record)    — real-time updates from SSE
    ▼
Live dashboard
```

## Crate Structure

### web-dashboard-common (the framework)

```
web-dashboard-common/
  src/
    lib.rs              # public API: run_dashboard(), EventMapper trait, EventRecord
    config.rs           # TOML config structs (serde)
    state.rs            # AppState, DashboardState, ActorStats
    handler.rs          # DashboardHandler (Kafka → SSE bridge)
    renderer.rs         # Askama template rendering, route generation
    widgets/
      mod.rs            # Widget enum dispatch
      stats_card.rs     # StatsCard widget template
      bar_chart.rs      # BarChart widget template
      pie_chart.rs      # PieChart widget template
      event_table.rs    # EventTable widget template
      custom.rs         # Custom widget (loads external JS/CSS)
  templates/
    page.html           # Askama base template (head, body shell, grid)
    widgets/
      stats_card.html   # Askama fragment — empty container with data- attrs
      bar_chart.html    # Askama fragment
      pie_chart.html    # Askama fragment
      event_table.html  # Askama fragment
      custom.html       # Askama fragment — includes <script> for custom JS
  static/
    css/
      dashboard.css     # Unified design system
    js/
      dashboard.js      # Widget renderer: discovers containers, inits Chart.js
      sse.js            # SSE connection with exponential backoff reconnection
      utils.js          # formatTime, shortenAddress, motesToCspr
```

### Dashboard crate (e.g., casper-exchange-monitor)

```
casper-exchange-monitor/
  Cargo.toml
  src/main.rs           # ExchangeMapper + main()
  dashboard.toml        # UI config
  static/js/custom/     # (optional) custom widget JS files
  static/css/custom/    # (optional) custom widget CSS files
```

## TOML Config Format

```toml
[service]
name = "casper-exchange-monitor"
web_port = 8080
prometheus_port = 9102
metric_name = "casper_exchange_events_total"
topics = ["apps.exchanges"]
group_id = "exchange-monitor-v1"
max_events = 200              # in-memory event buffer size (default: 100)
broadcast_capacity = 100      # SSE broadcast channel size (default: 100)

[theme]
accent = "#f59e0b"            # override accent color
colors = { inflow = "#22c55e", outflow = "#ef4444" }  # action color overrides

[[pages]]
path = "/"
title = "Casper Exchange Monitor"
subtitle = "Live inflow / outflow tracking for monitored exchanges"

[[pages.widgets]]
type = "bar_chart"
id = "exchange-volume"
row = 1
col = 1
title = "Exchange Volume"
datasets = [
  { field = "action", values = ["inflow", "outflow"], group_by = "actor" }
]

[[pages.widgets]]
type = "stats_cards"
id = "exchange-stats"
row = 1
col = 2
group_by = "actor"
metrics = ["tx_count", "total_amount"]

[[pages.widgets]]
type = "event_table"
id = "event-log"
row = 2
col = 1
width = 2
columns = ["timestamp", "actor", "action", "amount", "target", "status", "tx_hash"]
max_rows = 20

[[pages]]
path = "/account/{address}"
title = "Account — {{ address }}"

[[pages.widgets]]
type = "custom"
id = "account-graph"
widget_key = "account_graph"
row = 1
col = 1
width = 2
js = "account-graph.js"
css = "account-graph.css"
```

### Widget grid layout

Widgets are placed using `row`, `col`, and `width` (column span, default 1). The framework determines the total column count from the maximum `col + width - 1` in each row. Default grid is 2 columns.

```toml
# Row 1: chart on left, stats on right
[[pages.widgets]]
row = 1
col = 1
# ...

[[pages.widgets]]
row = 1
col = 2
# ...

# Row 2: full-width table
[[pages.widgets]]
row = 2
col = 1
width = 2
# ...
```

## Widget Rendering Contract

Each widget type in `dashboard.js` implements three functions:

```js
widgets.bar_chart = {
  init(el, config)         // Create empty Chart.js structure inside el
  bootstrap(el, stats)     // Populate from /api/stats response
  update(el, eventRecord)  // Handle a single SSE event
};
```

**Lifecycle:**
1. Page loads → `dashboard.js` calls `init()` for each `[data-widget]` container
2. Fetches `GET /api/stats` → calls `bootstrap()` for each widget
3. Connects to `GET /events` SSE → calls `update()` for each widget on every event

**Custom widgets** register themselves using the `widget_key` specified in the TOML config:

```toml
[[pages.widgets]]
type = "custom"
id = "account-graph"
widget_key = "account_graph"   # JS registration key (required for custom widgets)
js = "account-graph.js"
```

```js
// account-graph.js
widgets.account_graph = {
  init(el, config) { /* D3 force graph setup */ },
  bootstrap(el, stats) { /* populate from API */ },
  update(el, record) { /* add new node/link */ }
};
```

The `widget_key` must be a valid JS identifier (alphanumeric + underscores). The framework validates this at startup.

## V1 Widget Catalog

| Widget | Config type | Renders | Data source |
|---|---|---|---|
| Stats Cards | `stats_cards` | Grouped metric cards (tx count, total amount per actor) | `stats.actors` |
| Bar Chart | `bar_chart` | Chart.js bar chart, grouped by actor/action | `stats.actors` + SSE |
| Pie Chart | `pie_chart` | Chart.js doughnut, grouped by action or actor | `stats.actors` |
| Event Table | `event_table` | Streaming table with configurable columns, badges, links | `stats.recent_events` + SSE |
| Custom | `custom` | External JS/CSS — escape hatch | Depends on implementation |

**Deferred to v2:** Bubble/scatter chart, line chart, gauge.

### Widget-to-struct mapping

| Widget | Bootstrap data | SSE update data | Key struct |
|---|---|---|---|
| Stats Cards | `stats.actors` → `ActorStats.tx_count`, `ActorStats.total_amount`, `ActorStats.actions` | Increments counts from `EventRecord` | `ActorStats` |
| Bar Chart | `stats.actors` → groups `ActorStats.actions` by actor | Adds `EventRecord.amount` to matching bar | `ActorStats` |
| Pie Chart | `stats.actors` → aggregates `ActorStats.actions` or `ActorStats.total_amount` | Adds `EventRecord.amount` to matching slice | `ActorStats` |
| Event Table | `stats.recent_events` → list of `EventRecord` | Prepends new `EventRecord` to table | `EventRecord` |
| Custom | `stats` (full response) | `EventRecord` | Depends on implementation |

## API Routes

All routes are auto-generated from the config. No `UiRouter` or `ApiRouter` traits.

### Standard endpoints (every dashboard)

| Endpoint | Purpose |
|---|---|
| `GET /` | Generated HTML for first page |
| `GET /{page_path}` | Generated HTML for each config-defined page |
| `GET /events` | SSE stream of `EventRecord` |
| `GET /api/stats` | Full state: `{ actors: HashMap<String, ActorStats>, recent_events: Vec<EventRecord> }` |
| `GET /api/config` | Parsed TOML config as JSON (debugging) |
| `GET /static/*` | Shared CSS/JS + dashboard-specific custom assets |
| `GET /health` | `{"status": "ok", "service": "<name>"}` |
| `GET /metrics` | Prometheus scrape endpoint |

### Parameterized page routes

When a page path contains a parameter (e.g., `/account/{address}`), the framework generates a corresponding API route:

| Config path | HTML route | API route |
|---|---|---|
| `/account/{address}` | `GET /account/:address` | `GET /api/account/:address` |

The framework provides a built-in filter for the `{address}` parameter:
- `{address}` → events where `actor_address == address` OR `target == address`, plus counterparty summary (same as current whale `AccountEventsResponse`)

Other parameter names (e.g., `{hash}`, `{validator}`) require the dashboard to define filtering via a TOML mapping or use the `ApiExtension` trait:

```toml
[[pages]]
path = "/validator/{hash}"
filter_field = "target"        # filter events where this EventRecord field == path param
```

If `filter_field` is not specified and the parameter is not `{address}`, the framework returns an error at startup.

### Optional API extension

For truly custom API logic beyond filtering (e.g., aggregations, joins with external data), an optional `ApiExtension` trait can add extra routes:

```rust
trait ApiExtension {
    fn extra_routes(&self) -> Router<AppState>;
}
```

Most dashboards will not need this.

## CSS Design System

A single shared stylesheet with CSS variables. Dashboards override via `[theme]` in TOML.

### Base variables

```css
:root {
  /* Surfaces */
  --bg-void: #0b0e13;
  --bg-plate: #121620;
  --bg-ridge: #1a1f2e;
  --bg-shelf: #232a3b;

  /* Text */
  --text-primary: #e8ecf4;
  --text-secondary: #8a94a6;
  --text-muted: #555f73;

  /* Accent (overridable per dashboard) */
  --accent: #00e5c8;
  --accent-dim: rgba(0, 229, 200, 0.15);

  /* Action colors (overridable) */
  --color-inflow: #22c55e;
  --color-outflow: #ef4444;
  --color-transfer: #38bdf8;
  --color-delegate: #a78bfa;
  --color-undelegate: #fb923c;
  --color-redelegate: #facc15;
  --color-bid: #34d399;
  --color-other: #64748b;

  /* Typography */
  --font-body: 'Outfit', sans-serif;
  --font-data: 'IBM Plex Mono', monospace;

  /* Layout */
  --grid-gap: 16px;
  --widget-radius: 12px;
  --widget-padding: 20px;
}
```

### Theme overrides

The framework injects TOML theme values as a `<style>` block in the generated HTML:

```html
<style>:root { --accent: #f59e0b; --color-inflow: #22c55e; }</style>
```

### Widget CSS

Each widget type has a scoped class (`.widget-bar-chart`, `.widget-stats-cards`, etc.) in the shared stylesheet. No per-dashboard CSS for standard widgets.

### Custom widget CSS

Custom widgets can bring their own CSS:

```toml
[[pages.widgets]]
type = "custom"
js = "account-graph.js"
css = "account-graph.css"
```

## Config Validation

`DashboardConfig::from_toml()` validates the config at startup and returns an error on failure. The caller (typically `main()`) decides whether to panic or handle gracefully. Validation rules:

- **Required fields**: `[service].name`, `web_port`, `prometheus_port`, `metric_name`, `topics`, `group_id` must all be present.
- **Pages**: At least one page must be defined. Each page must have a `path` and `title`.
- **Widgets**: Each widget must have `type`, `id`, `row`, `col`. The `id` must be unique within the page.
- **Widget types**: Must be one of `stats_cards`, `bar_chart`, `pie_chart`, `event_table`, `custom`. Unknown types cause a startup panic.
- **Custom widgets**: Must have `widget_key` (valid JS identifier: `^[a-zA-Z_][a-zA-Z0-9_]*$`) and `js` path. The JS file must exist at the expected location.
- **Parameterized pages**: If path contains a parameter other than `{address}`, `filter_field` must be specified.
- **Grid layout**: `row` and `col` must be >= 1. `width` defaults to 1.

Errors are formatted as: `dashboard config error: <field path>: <message>` (e.g., `dashboard config error: pages[0].widgets[1].type: unknown widget type "gauge"`).

## Static File Serving

The framework serves static files from two sources, merged into a single `/static/` route:

1. **Framework assets** (CSS design system, shared JS) — from `web-dashboard-common/static/`
2. **Dashboard-specific assets** (custom widget JS/CSS) — from `{dashboard_crate}/static/`

Dashboard files take precedence over framework files at the same path (allows overriding shared assets if needed).

**In development**: `run_dashboard` resolves paths relative to `CARGO_MANIFEST_DIR` of the calling crate and the `web-dashboard-common` crate.

**In Docker**: The Dockerfile copies both directories into `/app/static/`, with dashboard files overlaying framework files. The server serves from `/app/static/`.

The `from_toml()` method accepts an optional `static_dir` override for cases where the default resolution doesn't work.

## Mapper Configuration

Mapper-specific configuration (e.g., the exchange monitor's `EXCHANGE_FILTER`) remains **environment variable driven** and is out of scope for the TOML config. The TOML config describes the UI; the mapper struct owns its domain logic and configuration.

This is intentional — mapper behavior is Rust code, and env vars are the standard mechanism for runtime configuration of Rust services. Mixing domain-specific mapper settings into the UI config would conflate concerns.

## Rust Public API

### EventMapper trait (unchanged)

```rust
pub trait EventMapper: Send + Sync + 'static {
    fn map(&self, event: &EnrichedEvent) -> Option<EventRecord>;
}
```

### DashboardConfig (simplified)

```rust
impl<M: EventMapper> DashboardConfig<M> {
    pub fn from_toml(path: &str, mapper: M) -> Result<Self>;
}
```

Replaces the current generic `DashboardConfig<M, U, A>` with PhantomData. No more `UiRouter` or `ApiRouter` type parameters.

### Entry point

```rust
pub async fn run_dashboard<M: EventMapper>(config: DashboardConfig<M>) -> Result<()>;
```

## Developer Workflow

Creating a new dashboard:

1. Create crate with `Cargo.toml`, `src/main.rs`, `dashboard.toml`
2. Write the TOML config — define service metadata, pages, widget grid
3. Implement `EventMapper` — the only Rust code needed (typically 20-50 lines)
4. Call `DashboardConfig::from_toml("dashboard.toml", MyMapper)?` and `run_dashboard(config).await`
5. Add crate to workspace `Cargo.toml` members
6. `cargo build` — done

No HTML, CSS, or JavaScript required for standard dashboards. Custom widgets only when needed.

## Migration Path

### Removed

- `UiRouter` trait and all implementations
- `ApiRouter` trait and all implementations
- `DashboardConfig` PhantomData generics (`_ui`, `_api`)
- Per-dashboard HTML files (`casper-exchange-monitor/src/dashboard.html`, `web-whale-activity/html/*.html`)
- Per-dashboard CSS files (all replaced by shared design system)
- Per-dashboard inline JavaScript

### Preserved

- `EventMapper` trait — unchanged
- `EventRecord` struct — unchanged (8 fields)
- `DashboardHandler` — unchanged (Kafka → SSE bridge)
- SSE event flow — unchanged
- Prometheus metrics integration — unchanged

### Modified

- `DashboardState` — accepts configurable `max_events` buffer size (was hardcoded to 50)
- `ActorStats` — adds `total_amount: u64` field (sum of all action amounts). Existing `actions: HashMap<String, u64>` stays for per-action breakdown. The `stats_cards` widget resolves `"total_amount"` to this field and `"tx_count"` to the existing count.
- `DashboardConfig` — fields change from `&'static str` to `String` since values are parsed from TOML at runtime. PhantomData generics removed.
- `broadcast::channel` capacity — configurable via `broadcast_capacity` in `[service]` (default: 100)

### New

- `config.rs` — TOML config parsing
- `renderer.rs` — Askama HTML generation from config
- `widgets/` module — widget type definitions for Askama
- `templates/` — Askama templates (page shell + widget fragments)
- `static/js/dashboard.js` — widget rendering engine
- `static/js/sse.js` — shared SSE connection logic
- `static/js/utils.js` — shared utility functions

### Whale activity custom widgets

The D3 force-directed graph on the account page becomes a custom widget:
- `web-whale-activity/static/js/custom/account-graph.js`
- `web-whale-activity/static/css/custom/account-graph.css`
- Referenced in `dashboard.toml` as `type = "custom"`
