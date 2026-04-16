(function () {
  const { motesToCspr, formatTime, shortenAddress } = DashboardUtils;

  const DEFAULT_COLUMNS = ['timestamp', 'actor', 'action', 'amount', 'target', 'status', 'tx_hash'];
  const LABELS = {
    timestamp: 'Time', actor: 'Actor', action: 'Action', amount: 'Amount (CSPR)',
    target: 'Target', status: 'Status', tx_hash: 'Tx',
  };

  function renderCell(col, r) {
    switch (col) {
      case 'timestamp': return formatTime(r.timestamp);
      case 'actor': return `<a href="https://cspr.live/account/${r.actor_address}" target="_blank" rel="noopener">${r.actor}</a>`;
      case 'action': return `<span class="badge badge-${r.action}">${r.action.toUpperCase()}</span>`;
      case 'amount': return `<span class="mono">${motesToCspr(r.amount, 2)}</span>`;
      case 'target': return `<span class="mono">${shortenAddress(r.target)}</span>`;
      case 'status': return `<span class="badge badge-${r.status}">${r.status}</span>`;
      case 'tx_hash': return `<a class="mono" href="https://cspr.live/transaction/${r.tx_hash}" target="_blank" rel="noopener">${r.tx_hash.slice(0, 10)}…</a>`;
      default: return r[col] || '';
    }
  }

  Dashboard.register('event_table', {
    init(el, config) {
      el._columns = (config.columns && config.columns.length) ? config.columns : DEFAULT_COLUMNS;
      el._max = config.max_rows || 20;
      const thead = el.querySelector('thead');
      thead.innerHTML = '<tr>' + el._columns.map((c) => `<th>${LABELS[c] || c}</th>`).join('') + '</tr>';
      el._tbody = el.querySelector('tbody');
    },
    bootstrap(el, stats) {
      el._tbody.innerHTML = '';
      const events = (stats.recent_events || []).slice(0, el._max);
      events.slice().reverse().forEach((r) => prependRow(el, r));
    },
    update(el, record) { prependRow(el, record); },
  });

  function prependRow(el, r) {
    const tr = document.createElement('tr');
    tr.innerHTML = el._columns.map((c) => `<td>${renderCell(c, r)}</td>`).join('');
    el._tbody.prepend(tr);
    while (el._tbody.rows.length > el._max) el._tbody.deleteRow(el._tbody.rows.length - 1);
  }
})();
