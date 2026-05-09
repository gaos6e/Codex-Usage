import React from 'react';
import type { TimeRangePreset, UsageFilters } from '../../shared/contracts';
import { useI18n } from '../i18n/I18nContext';
import { SelectMenu } from './SelectMenu';

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
      <label className="select-field">
        {t('filter.range')}
        <SelectMenu
          ariaLabel={t('filter.range')}
          value={filters.range.preset}
          options={presets.map((preset) => ({ value: preset, label: t(`timeRange.${preset}`) }))}
          onChange={(preset) => onFiltersChange({
            ...filters,
            range: { ...filters.range, preset },
          })}
        />
      </label>

      {filters.range.preset === 'custom' ? (
        <div className="custom-date-fields">
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
        </div>
      ) : null}

      <label className="select-field">
        {t('filter.aggregation')}
        <SelectMenu
          ariaLabel={t('filter.aggregation')}
          value={filters.range.aggregation || 'daily'}
          options={[
            { value: 'daily', label: t('filter.daily') },
            { value: 'weekly', label: t('filter.weekly') },
          ]}
          onChange={(aggregation) => onFiltersChange({
            ...filters,
            range: { ...filters.range, aggregation },
          })}
        />
      </label>

      {showViewToggle ? (
        <div className="segmented" role="group" aria-label={t('view.metric')}>
          <button
            type="button"
            className={filters.view === 'time' ? 'selected' : ''}
            aria-pressed={filters.view === 'time'}
            onClick={() => onFiltersChange({ ...filters, view: 'time' })}
          >
            {t('view.time')}
          </button>
          <button
            type="button"
            className={filters.view === 'tokens' ? 'selected' : ''}
            aria-pressed={filters.view === 'tokens'}
            onClick={() => onFiltersChange({ ...filters, view: 'tokens' })}
          >
            {t('view.tokens')}
          </button>
        </div>
      ) : null}
    </>
  );
}
