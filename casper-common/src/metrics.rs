pub fn install_prometheus_exporter(port: u16) {
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], port))
        .install()
        .expect("Failed to install Prometheus exporter");
}
