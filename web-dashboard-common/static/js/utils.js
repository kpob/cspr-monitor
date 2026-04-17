(function (global) {
  const M = 1_000_000_000;

  function motesToCspr(motes, digits = 0) {
    return (motes / M).toLocaleString(undefined, { maximumFractionDigits: digits });
  }

  function csprNum(motes) {
    return +(motes / M).toFixed(0);
  }

  function formatTime(iso) {
    try {
      return new Date(iso).toLocaleTimeString([], {
        hour: '2-digit', minute: '2-digit', second: '2-digit',
      });
    } catch (_) { return iso; }
  }

  function shortenAddress(addr) {
    if (!addr) return '';
    return addr.length > 16 ? addr.slice(0, 8) + '..' + addr.slice(-6) : addr;
  }

  const ACTION_META = {
    inflow:          { label: 'Inflow',       color: 'var(--color-inflow)',      cls: 'inflow' },
    outflow:         { label: 'Outflow',      color: 'var(--color-outflow)',     cls: 'outflow' },
    native_transfer: { label: 'Transfer',     color: 'var(--color-transfer)',    cls: 'transfer' },
    transfer_in:     { label: 'Transfer In',  color: 'var(--color-transfer)',    cls: 'transfer' },
    transfer_out:    { label: 'Transfer Out', color: 'var(--color-transfer)',    cls: 'transfer' },
    delegation:      { label: 'Delegate',     color: 'var(--color-delegate)',    cls: 'delegation' },
    undelegation:    { label: 'Undelegate',   color: 'var(--color-undelegate)',  cls: 'undelegation' },
    redelegation:    { label: 'Redelegate',   color: 'var(--color-redelegate)',  cls: 'redelegation' },
    add_bid:         { label: 'Add Bid',      color: 'var(--color-bid)',         cls: 'bid' },
    withdraw_bid:    { label: 'Withdraw Bid', color: 'var(--color-bid)',         cls: 'bid' },
    activate_bid:    { label: 'Activate Bid', color: 'var(--color-bid)',         cls: 'bid' },
    session:         { label: 'Session',      color: 'var(--color-other)',       cls: 'other' },
  };

  function actionMeta(action) {
    if (!action) return { label: 'Unknown', color: 'var(--color-other)', cls: 'other' };
    if (action.startsWith('contract:')) {
      return { label: action.slice(9), color: 'var(--color-other)', cls: 'other' };
    }
    return ACTION_META[action] || { label: action, color: 'var(--color-other)', cls: 'other' };
  }

  function cssVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#888';
  }

  global.DashboardUtils = { motesToCspr, csprNum, formatTime, shortenAddress, actionMeta, cssVar, M };
})(window);
