/** Byte and duration formatting shared by the views. */

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

export function bytes(value, decimals = 1) {
  if (value === null || value === undefined || Number.isNaN(value)) return '—';
  if (value === 0) return '0 B';

  const exponent = Math.min(
    Math.floor(Math.log(Math.abs(value)) / Math.log(1024)),
    UNITS.length - 1
  );
  const scaled = value / 1024 ** exponent;
  // Whole numbers read better without a trailing .0 on the small units.
  return `${scaled.toFixed(exponent === 0 ? 0 : decimals)} ${UNITS[exponent]}`;
}

export function bytesPerSecond(value) {
  if (value === null || value === undefined) return '—';
  return `${bytes(value)}/s`;
}

export function percent(value, decimals = 1) {
  if (value === null || value === undefined || Number.isNaN(value)) return '—';
  return `${Number(value).toFixed(decimals)}%`;
}

/** Compact uptime: `3d 4h`, `4h 12m`, `12m`. */
export function duration(seconds) {
  if (!seconds && seconds !== 0) return '—';

  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

/** Threshold colour shared by every meter, so they read consistently. */
export function loadColor(value, { warn = 70, danger = 90 } = {}) {
  const n = Number(value);
  if (Number.isNaN(n)) return 'surface-variant';
  if (n >= danger) return 'error';
  if (n >= warn) return 'warning';
  return 'success';
}
