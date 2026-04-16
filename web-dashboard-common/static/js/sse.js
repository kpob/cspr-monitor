(function (global) {
  function connectSSE(url, { onMessage, onStatus }) {
    let sse = null;
    let delay = 1000;

    function open() {
      if (sse) sse.close();
      sse = new EventSource(url);
      sse.onopen = () => { delay = 1000; onStatus && onStatus('live'); };
      sse.onmessage = (e) => {
        let record;
        try { record = JSON.parse(e.data); } catch (_) { return; }
        onMessage && onMessage(record);
      };
      sse.onerror = () => {
        onStatus && onStatus('offline');
        sse.close();
        setTimeout(() => { delay = Math.min(delay * 2, 30000); open(); }, delay);
      };
    }

    open();
    return { close: () => sse && sse.close() };
  }

  global.DashboardSSE = { connectSSE };
})(window);
