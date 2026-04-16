(function () {
  const { csprNum, cssVar } = DashboardUtils;

  function actionColor(action) {
    const map = {
      inflow: 'color-inflow',
      outflow: 'color-outflow',
      native_transfer: 'color-transfer',
      delegation: 'color-delegate',
      undelegation: 'color-undelegate',
      redelegation: 'color-redelegate',
      add_bid: 'color-bid',
      withdraw_bid: 'color-bid',
    };
    return cssVar('--' + (map[action] || 'color-other'));
  }

  function rebuild(chart, actors, actions) {
    const labels = Object.keys(actors);
    chart.data.labels = labels;
    chart.data.datasets = actions.map((action) => ({
      label: action,
      data: labels.map((a) => csprNum(actors[a].actions[action] || 0)),
      backgroundColor: actionColor(action),
      borderWidth: 0,
    }));
    chart.update('none');
  }

  Dashboard.register('bar_chart', {
    init(el, config) {
      const canvas = el.querySelector('canvas');
      el._chart = new Chart(canvas.getContext('2d'), {
        type: 'bar',
        data: { labels: [], datasets: [] },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          scales: {
            x: { ticks: { color: cssVar('--text-muted') }, grid: { color: cssVar('--bg-ridge') } },
            y: { ticks: { color: cssVar('--text-muted') }, grid: { color: cssVar('--bg-ridge') }, beginAtZero: true },
          },
          plugins: { legend: { labels: { color: cssVar('--text-primary'), font: { size: 11 } } } },
        },
      });
      const ds = config.datasets && config.datasets[0];
      el._actions = (ds && ds.values && ds.values.length) ? ds.values : ['inflow', 'outflow'];
      el._actors = {};
    },
    bootstrap(el, stats) {
      el._actors = JSON.parse(JSON.stringify(stats.actors || {}));
      rebuild(el._chart, el._actors, el._actions);
    },
    update(el, record) {
      const a = el._actors[record.actor] || { actions: {}, tx_count: 0, total_amount: 0 };
      a.tx_count += 1;
      a.total_amount += record.amount;
      a.actions[record.action] = (a.actions[record.action] || 0) + record.amount;
      el._actors[record.actor] = a;
      rebuild(el._chart, el._actors, el._actions);
    },
  });
})();
