(function () {
  const { shortenAddress, motesToCspr, actionMeta, M } = DashboardUtils;

  function dominantAction(actions) {
    let max = 0, best = 'unknown';
    for (const [a, v] of Object.entries(actions || {})) {
      if (v > max) { max = v; best = a; }
    }
    return best;
  }

  function addressFromPath() {
    return window.location.pathname.split('/account/')[1] || '';
  }

  function build(el, data) {
    const container = el.querySelector('.custom-body');
    const width = container.clientWidth || 800;
    const height = Math.max(420, container.clientHeight || 420);
    container.innerHTML = '';
    const address = addressFromPath();

    const entries = Object.entries(data.targets || {});
    if (entries.length === 0) {
      container.innerHTML = '<p class="empty-msg">No counterparties yet.</p>';
      return;
    }

    const nodes = [{ id: address, label: shortenAddress(address), isCenter: true, radius: 22 }];
    const links = [];
    entries.forEach(([addr, info]) => {
      const vol = info.total_amount / M;
      const r = Math.max(8, Math.min(18, 4 + Math.sqrt(vol / 1000) * 2));
      nodes.push({ id: addr, label: addr === 'self' ? 'self' : shortenAddress(addr), radius: r, info });
      links.push({ source: address, target: addr, value: info.tx_count, amount: info.total_amount, action: dominantAction(info.actions) });
    });

    const svg = d3.select(container).append('svg').attr('width', width).attr('height', height);
    const g = svg.append('g');

    const link = g.selectAll('line').data(links).join('line')
      .attr('stroke', (d) => actionMeta(d.action).color)
      .attr('stroke-opacity', 0.5)
      .attr('stroke-width', (d) => Math.max(1.5, Math.min(6, d.value * 1.5)));

    const linkLabel = g.selectAll('.link-label').data(links).join('text')
      .attr('class', 'link-label')
      .text((d) => motesToCspr(d.amount) + ' CSPR');

    const node = g.selectAll('circle').data(nodes, (d) => d.id).join('circle')
      .attr('r', (d) => d.radius)
      .attr('fill', (d) => d.isCenter ? 'var(--accent)' : actionMeta(dominantAction(d.info && d.info.actions)).color)
      .attr('stroke', (d) => d.isCenter ? 'var(--accent)' : actionMeta(dominantAction(d.info && d.info.actions)).color)
      .attr('stroke-width', 1.5)
      .attr('cursor', (d) => d.isCenter ? 'default' : 'pointer')
      .on('click', (_e, d) => { if (!d.isCenter && d.id !== 'self') window.location.href = '/account/' + d.id; });

    const label = g.selectAll('.node-label').data(nodes, (d) => d.id).join('text')
      .attr('class', (d) => 'node-label' + (d.isCenter ? ' center' : ''))
      .text((d) => d.label)
      .attr('dy', (d) => d.radius + 13);

    const cx = width / 2, cy = height / 2;
    const sim = d3.forceSimulation(nodes)
      .force('link', d3.forceLink(links).id((d) => d.id).distance(140))
      .force('charge', d3.forceManyBody().strength(-300))
      .force('center', d3.forceCenter(cx, cy))
      .force('collide', d3.forceCollide().radius((d) => d.radius + 20))
      .on('tick', () => {
        nodes[0].fx = cx; nodes[0].fy = cy;
        link
          .attr('x1', (d) => d.source.x).attr('y1', (d) => d.source.y)
          .attr('x2', (d) => d.target.x).attr('y2', (d) => d.target.y);
        linkLabel
          .attr('x', (d) => (d.source.x + d.target.x) / 2)
          .attr('y', (d) => (d.source.y + d.target.y) / 2 - 6);
        node.attr('cx', (d) => d.x).attr('cy', (d) => d.y);
        label.attr('x', (d) => d.x).attr('y', (d) => d.y);
      });
    el._sim = sim;
    el._data = data;
  }

  Dashboard.register('account_graph', {
    init(el) {
      const address = addressFromPath();
      el._address = address;
      fetch('/api/account/' + address)
        .then((r) => r.ok ? r.json() : { targets: {} })
        .then((data) => build(el, data))
        .catch(() => build(el, { targets: {} }));
    },
    bootstrap() { /* handled inside init */ },
    update(el, record) {
      const address = el._address;
      if (record.actor_address !== address && record.target !== address) return;
      const data = el._data || { targets: {} };
      const other = record.actor_address === address ? record.target : record.actor_address;
      const t = data.targets[other] || { tx_count: 0, total_amount: 0, actions: {} };
      t.tx_count += 1;
      t.total_amount += record.amount;
      t.actions[record.action] = (t.actions[record.action] || 0) + record.amount;
      data.targets[other] = t;
      build(el, data);
    },
  });

  if (!window.d3) {
    const s = document.createElement('script');
    s.src = 'https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js';
    document.head.appendChild(s);
  }
})();
