(function () {
  const { motesToCspr } = DashboardUtils;

  function card(label, value) {
    return `<div class="stats-card"><div class="label">${label}</div><div class="value">${value}</div></div>`;
  }

  function renderAll(body, stats, config) {
    const actors = stats.actors || {};
    const metrics = (config.metrics && config.metrics.length ? config.metrics : ['tx_count', 'total_amount']);
    const rows = Object.entries(actors).map(([name, s]) => {
      const cells = metrics.map((m) => {
        if (m === 'tx_count') return card(`${name} · tx`, s.tx_count || 0);
        if (m === 'total_amount') return card(`${name} · CSPR`, motesToCspr(s.total_amount || 0));
        return card(`${name} · ${m}`, '—');
      }).join('');
      return cells;
    }).join('');
    body.innerHTML = rows || '<div class="stats-card"><div class="label">No data yet</div><div class="value">—</div></div>';
  }

  Dashboard.register('stats_cards', {
    init(el) { el._body = el.querySelector('.stats-cards-body'); },
    bootstrap(el, stats, config) { renderAll(el._body, stats, config); },
    update(el, record, config) {
      if (!el._stats) el._stats = { actors: {}, recent_events: [] };
      const a = el._stats.actors[record.actor] || { actions: {}, tx_count: 0, total_amount: 0 };
      a.tx_count += 1;
      a.total_amount += record.amount;
      a.actions[record.action] = (a.actions[record.action] || 0) + record.amount;
      el._stats.actors[record.actor] = a;
      renderAll(el._body, el._stats, config);
    },
  });
})();
