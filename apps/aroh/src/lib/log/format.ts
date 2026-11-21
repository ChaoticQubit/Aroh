/** Formats an amount with its unit, dropping trailing zeros: `500 ml`, `1.5 h`, `30 min`. */
export function formatAmount(amount: number, unit: string): string {
  const rounded = Math.round(amount * 10) / 10;
  return `${rounded.toLocaleString()} ${unit}`.trim();
}

/** Formats a UTC ISO 8601 timestamp as local wall-clock time, such as `14:05`. */
export function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}
