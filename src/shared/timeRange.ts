import type { AggregationMode, TimeRangeFilter } from './contracts';

const DAY_MS = 24 * 60 * 60 * 1000;

export interface ResolvedRange {
  start: Date;
  end: Date;
  label: string;
  aggregation: AggregationMode;
}

export function startOfLocalDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

export function parseDateInput(value: string | undefined, fallback: Date): Date {
  if (!value) {
    return fallback;
  }
  const parts = value.split('-').map((part) => Number(part));
  if (parts.length !== 3 || parts.some((part) => Number.isNaN(part))) {
    return fallback;
  }
  return new Date(parts[0], parts[1] - 1, parts[2]);
}

export function resolveTimeRange(
  filter: TimeRangeFilter,
  allTimeStart?: Date,
  allTimeEnd?: Date,
  now = new Date(),
): ResolvedRange {
  const today = startOfLocalDay(now);
  let start = today;
  let end = addDays(today, 1);
  let label = 'Today';

  switch (filter.preset) {
    case 'last7':
      start = addDays(today, -6);
      label = 'Last 7 days';
      break;
    case 'last30':
      start = addDays(today, -29);
      label = 'Last 30 days';
      break;
    case 'last90':
      start = addDays(today, -89);
      label = 'Last 90 days';
      break;
    case 'all':
      start = allTimeStart ? startOfLocalDay(allTimeStart) : today;
      end = allTimeEnd ? addDays(startOfLocalDay(allTimeEnd), 1) : addDays(today, 1);
      label = 'All time';
      break;
    case 'custom':
      start = parseDateInput(filter.startDate, today);
      end = addDays(parseDateInput(filter.endDate, today), 1);
      if (end <= start) {
        end = addDays(start, 1);
      }
      label = 'Custom range';
      break;
    case 'today':
    default:
      break;
  }

  const days = Math.max(1, Math.ceil((end.getTime() - start.getTime()) / DAY_MS));
  const aggregation = filter.aggregation || (days > 45 ? 'weekly' : 'daily');
  return { start, end, label, aggregation };
}

export function isInsideRange(date: Date, range: ResolvedRange): boolean {
  const time = date.getTime();
  return time >= range.start.getTime() && time < range.end.getTime();
}

export function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function weekKey(date: Date): string {
  const day = startOfLocalDay(date);
  const offset = (day.getDay() + 6) % 7;
  const monday = addDays(day, -offset);
  return localDateKey(monday);
}

export function bucketKey(date: Date, aggregation: AggregationMode): string {
  return aggregation === 'weekly' ? weekKey(date) : localDateKey(date);
}

export function bucketLabel(key: string, aggregation: AggregationMode): string {
  if (aggregation === 'weekly') {
    return `Week of ${key.slice(5)}`;
  }
  return key.slice(5);
}
