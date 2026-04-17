(function () {
  const { csprNum, cssVar, actionMeta } = DashboardUtils;

  function rebuild(chart, totals) {
    const entries = Object.entries(totals).filter(([, v]) => v > 0).sort((a, b) => b[1] - a[1]);
    chart.data.labels = entries.map(([a]) => actionMeta(a).label);
    chart.data.datasets[0].data = entries.map(([, v]) => csprNum(v));
    chart.data.datasets[0].backgroundColor = entries.map(([a]) => {
      const meta = actionMeta(a);
      const raw = meta.color;
      if (raw.startsWith('var(')) return cssVar(raw.slice(4, -1));
      return raw;
    });
    chart.update('none');
  }

  Dashboard.register('pie_chart', {
    init(el) {
      const canvas = el.querySelector('canvas');
      el._chart = new Chart(canvas.getContext('2d'), {
        type: 'doughnut',
        data: { labels: [], datasets: [{ data: [], backgroundColor: [], borderColor: cssVar('--bg-plate'), borderWidth: 2 }] },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          cutout: '60%',
          plugins: { legend: { position: 'bottom', labels: { color: cssVar('--text-muted'), font: { size: 10 } } } },
        },
      });
      el._totals = {};
    },
    bootstrap(el, stats) {
      el._totals = {};
      for (const s of Object.values(stats.actors || {})) {
        for (const [action, amount] of Object.entries(s.actions || {})) {
          el._totals[action] = (el._totals[action] || 0) + amount;
        }
      }
      rebuild(el._chart, el._totals);
    },
    update(el, record) {
      el._totals[record.action] = (el._totals[record.action] || 0) + record.amount;
      rebuild(el._chart, el._totals);
    },
  });
})();
