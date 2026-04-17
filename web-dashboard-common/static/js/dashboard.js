(function (global) {
  const widgets = {};

  function register(kind, impl) { widgets[kind] = impl; }

  async function start(statsUrl, eventsUrl) {
    const containers = Array.from(document.querySelectorAll('[data-widget]'));
    const instances = containers.map((el) => {
      const kind = el.dataset.widget;
      const config = JSON.parse(el.dataset.widgetConfig || '{}');
      const impl = widgets[kind] || (config.widget_key && widgets[config.widget_key]);
      if (!impl) { console.warn('No widget impl for', kind, config.widget_key); return null; }
      try { impl.init && impl.init(el, config); } catch (e) { console.error('init failed', kind, e); }
      return { el, impl, config };
    }).filter(Boolean);

    try {
      const res = await fetch(statsUrl);
      if (res.ok) {
        const stats = await res.json();
        for (const { el, impl, config } of instances) {
          try { impl.bootstrap && impl.bootstrap(el, stats, config); } catch (e) { console.error('bootstrap failed', e); }
        }
      }
    } catch (e) { console.warn('bootstrap fetch failed', e); }

    const dot = document.getElementById('connDot');
    const txt = document.getElementById('connText');
    DashboardSSE.connectSSE(eventsUrl, {
      onStatus: (s) => {
        if (!dot) return;
        dot.classList.toggle('off', s === 'offline');
        if (txt) txt.textContent = s === 'live' ? 'Live' : 'Reconnecting…';
      },
      onMessage: (record) => {
        for (const { el, impl, config } of instances) {
          try { impl.update && impl.update(el, record, config); } catch (e) { console.error('update failed', e); }
        }
      },
    });
  }

  global.Dashboard = { register, start, widgets };
})(window);
