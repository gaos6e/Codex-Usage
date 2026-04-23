export function formatInteger(value: number): string {
  return Math.round(value || 0).toLocaleString();
}

export function formatTokens(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}K`;
  }
  return formatInteger(value);
}

export function formatDecimal(value: number, maximumFractionDigits = 1): string {
  return (value || 0).toLocaleString(undefined, {
    maximumFractionDigits,
  });
}

export function formatPercent(value: number): string {
  const percent = Math.max(0, value || 0) * 100;
  return `${formatDecimal(percent)}%`;
}

export function formatDuration(ms: number): string {
  const totalMinutes = Math.max(0, Math.round((ms || 0) / 60000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours <= 0) {
    return `${minutes}m`;
  }
  if (minutes === 0) {
    return `${hours}h`;
  }
  return `${hours}h ${minutes}m`;
}

export function formatDateTime(value: string | undefined): string {
  if (!value) {
    return 'Unavailable';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return 'Unavailable';
  }
  return date.toLocaleString();
}

export function clampNumber(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
