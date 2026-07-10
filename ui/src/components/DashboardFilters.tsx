import { CalendarRange, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type {
  ArchiveFilter,
  DashboardSnapshot,
  RangePreset,
  UsageFilters,
} from '../types';

const presets: Array<{ value: RangePreset; label: string }> = [
  { value: 'today', label: '今日' },
  { value: 'last24_hours', label: '24 小时' },
  { value: 'last7_days', label: '7 天' },
  { value: 'last14_days', label: '14 天' },
  { value: 'last30_days', label: '30 天' },
  { value: 'last90_days', label: '90 天' },
  { value: 'all', label: '全部' },
  { value: 'custom', label: '自定义' },
];

export function cascadeFilter(
  current: UsageFilters,
  field: 'workspaceId' | 'modelProvider' | 'model',
  value?: string,
): UsageFilters {
  if (field === 'workspaceId') {
    return { ...current, workspaceId: value, modelProvider: undefined, model: undefined };
  }
  if (field === 'modelProvider') {
    return { ...current, modelProvider: value, model: undefined };
  }
  return { ...current, model: value };
}

interface DashboardFiltersProps {
  filters: UsageFilters;
  options?: DashboardSnapshot['filterOptions'];
  refreshing: boolean;
  onChange: (filters: UsageFilters) => void;
  onRefresh: () => void;
  onManageWorkspaces?: () => void;
}

export function DashboardFilters({
  filters,
  options,
  refreshing,
  onChange,
  onRefresh,
  onManageWorkspaces = () => undefined,
}: DashboardFiltersProps) {
  const { t } = useTranslation();
  const selectRange = (preset: RangePreset) => {
    const nextRange =
      preset === 'custom'
        ? {
            preset,
            startMs: filters.range.startMs ?? Date.now() - 7 * 86_400_000,
            endMs: filters.range.endMs ?? Date.now(),
            liveEnd: filters.range.liveEnd,
          }
        : { preset, liveEnd: false };
    onChange({ ...filters, range: nextRange });
  };

  const updateCustomTime = (field: 'startMs' | 'endMs', value: string) => {
    const timestamp = new Date(value).getTime();
    if (!Number.isFinite(timestamp)) return;
    onChange({
      ...filters,
      range: { ...filters.range, preset: 'custom', [field]: timestamp },
    });
  };

  return (
    <section className="filter-card" aria-label={t('全局筛选')}>
      <div className="preset-row" role="group" aria-label={t('时间范围')}>
        {presets.map((preset) => (
          <button
            key={preset.value}
            type="button"
            className={filters.range.preset === preset.value ? 'preset active' : 'preset'}
            aria-pressed={filters.range.preset === preset.value}
            onClick={() => selectRange(preset.value)}
          >
            {t(preset.label)}
          </button>
        ))}
      </div>

      <div className="select-row">
        <label>
          <span>{t('工作区')}</span>
          <select
            value={filters.workspaceId ?? ''}
            onChange={(event) => {
              if (event.target.value === '__manage_workspaces__') {
                onManageWorkspaces();
                return;
              }
              onChange(cascadeFilter(filters, 'workspaceId', event.target.value || undefined));
            }}
          >
            <option value="">{t('全部工作区')}</option>
            {options?.workspaces.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
            <option value="__manage_workspaces__">{t('其他工作区…')}</option>
          </select>
        </label>
        <label>
          <span>{t('提供方')}</span>
          <select
            value={filters.modelProvider ?? ''}
            onChange={(event) =>
              onChange(cascadeFilter(filters, 'modelProvider', event.target.value || undefined))
            }
          >
            <option value="">{t('全部提供方')}</option>
            {options?.providers.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </label>
        <label>
          <span>{t('模型')}</span>
          <select
            value={filters.model ?? ''}
            onChange={(event) =>
              onChange(cascadeFilter(filters, 'model', event.target.value || undefined))
            }
          >
            <option value="">{t('全部模型')}</option>
            {options?.models.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
        </label>
        <label>
          <span>{t('会话状态')}</span>
          <select
            value={filters.archived}
            onChange={(event) =>
              onChange({ ...filters, archived: event.target.value as ArchiveFilter })
            }
          >
            <option value="all">{t('活动与归档')}</option>
            <option value="active">{t('仅活动')}</option>
            <option value="archived">{t('仅归档')}</option>
          </select>
        </label>
        <button
          type="button"
          className="refresh-button"
          aria-label={t('手动刷新')}
          onClick={onRefresh}
          disabled={refreshing}
        >
          <RefreshCw className={refreshing ? 'spin' : ''} />
          {t('刷新')}
        </button>
      </div>

      {filters.range.preset === 'custom' && (
        <div className="custom-range-row">
          <CalendarRange aria-hidden="true" />
          <label>
            <span>{t('开始')}</span>
            <input
              type="datetime-local"
              value={toLocalInput(filters.range.startMs)}
              onChange={(event) => updateCustomTime('startMs', event.target.value)}
            />
          </label>
          <label>
            <span>{t('结束')}</span>
            <input
              type="datetime-local"
              value={toLocalInput(filters.range.endMs)}
              onChange={(event) => updateCustomTime('endMs', event.target.value)}
              disabled={filters.range.liveEnd}
            />
          </label>
          <label className="live-end-toggle">
            <input
              type="checkbox"
              checked={filters.range.liveEnd}
              onChange={(event) =>
                onChange({
                  ...filters,
                  range: { ...filters.range, liveEnd: event.target.checked },
                })
              }
            />
            {t('结束时间跟随现在')}
          </label>
        </div>
      )}
    </section>
  );
}

function toLocalInput(timestamp?: number): string {
  if (timestamp == null) return '';
  const date = new Date(timestamp);
  const local = new Date(timestamp - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}
