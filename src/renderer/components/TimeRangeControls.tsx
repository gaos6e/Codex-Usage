import React from 'react';
import type { AggregationMode, TimeRangePreset, UsageFilters } from '../../shared/contracts';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  filters: UsageFilters;
  onFiltersChange: (filters: UsageFilters) => void;
  showViewToggle?: boolean;
}

const presets: TimeRangePreset[] = ['today', 'last7', 'last30', 'last90', 'all', 'custom'];

export function TimeRangeControls({ filters, onFiltersChange, showViewToggle = true }: Props): React.ReactElement {
  const { t } = useI18n();

  return (
    <>
      <label>
        {t('filter.range')}
        <select
          value={filters.range.preset}
          onChange={(event) => onFiltersChange({
            ...filters,
            range: { ...filters.range, preset: event.target.value as TimeRangePreset },
          })}
        >
          {presets.map((preset) => <option key={preset} value={preset}>{t(`timeRange.${preset}`)}</option>)}
        </select>
      </label>

      {filters.range.preset === 'custom' ? (
        <>
          <label>
            {t('filter.start')}
            <input
              type="date"
              value={filters.range.startDate || ''}
              onChange={(event) => onFiltersChange({
                ...filters,
                range: { ...filters.range, startDate: event.target.value },
              })}
            />
          </label>
          <label>
            {t('filter.end')}
            <input
              type="date"
              value={filters.range.endDate || ''}
              onChange={(event) => onFiltersChange({
                ...filters,
                range: { ...filters.range, endDate: event.target.value },
              })}
            />
          </label>
        </>
      ) : null}

      <label>
        {t('filter.aggregation')}
        <select
          value={filters.range.aggregation || 'daily'}
          onChange={(event) => onFiltersChange({
            ...filters,
            range: { ...filters.range, aggregation: event.target.value as AggregationMode },
          })}
        >
          <option value="daily">{t('filter.daily')}</option>
          <option value="weekly">{t('filter.weekly')}</option>
        </select>
      </label>

      {showViewToggle ? (
        <div className="segmented" role="tablist" aria-label={t('view.metric')}>
          <button
            type="button"
            className={filters.view === 'time' ? 'selected' : ''}
            onClick={() => onFiltersChange({ ...filters, view: 'time' })}
          >
            {t('view.time')}
          </button>
          <button
            type="button"
            className={filters.view === 'tokens' ? 'selected' : ''}
            onClick={() => onFiltersChange({ ...filters, view: 'tokens' })}
          >
            {t('view.tokens')}
          </button>
        </div>
      ) : null}
    </>
  );
}
