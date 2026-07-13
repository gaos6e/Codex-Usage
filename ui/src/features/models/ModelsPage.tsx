import { useState } from 'react';
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { CircleDollarSign, CloudDownload, RotateCcw, Save, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  applyPriceUpdate,
  deleteModelPrice,
  getModels,
  isTauriRuntime,
  listModelPrices,
  previewPriceUpdate,
  restoreBuiltinPrice,
  saveModelPrice,
} from '../../api';
import { PageControls } from '../../components/PageControls';
import { QueryStatus } from '../../components/QueryStatus';
import { formatCost, formatTokens } from '../../lib/format';
import type { ModelPriceInput, UsageFilters } from '../../types';

const EMPTY_PRICE: ModelPriceInput = {
  provider: 'openai', pricingId: '', displayName: '', inputPerMillionUsd: '',
  outputPerMillionUsd: '', cacheReadPerMillionUsd: '', cacheWritePerMillionUsd: undefined,
};

export function ModelsPage({ filters }: { filters: UsageFilters }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<'usage' | 'prices'>('usage');
  const [search, setSearch] = useState('');
  const [sort, setSort] = useState('name');
  const [descending, setDescending] = useState(true);
  const [page, setPage] = useState(0);
  const [editing, setEditing] = useState<ModelPriceInput>();
  const [updatePreview, setUpdatePreview] = useState<Awaited<ReturnType<typeof previewPriceUpdate>>>();
  const request = { filters, search, sort, descending, page, pageSize: 30 };
  const models = useQuery({ queryKey: ['models', request], queryFn: () => getModels(request), placeholderData: keepPreviousData });
  const prices = useQuery({ queryKey: ['model-prices'], queryFn: () => listModelPrices(true) });
  const finishMutation = () => {
    setEditing(undefined);
    void queryClient.invalidateQueries({ queryKey: ['model-prices'] });
    void queryClient.invalidateQueries({ queryKey: ['models'] });
    void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
  };
  const save = useMutation({ mutationFn: (input: ModelPriceInput) => saveModelPrice(input), onSuccess: finishMutation });
  const remove = useMutation({ mutationFn: ({ provider, pricingId }: { provider: string; pricingId: string }) => deleteModelPrice(provider, pricingId), onSuccess: finishMutation });
  const restore = useMutation({ mutationFn: ({ provider, pricingId }: { provider: string; pricingId: string }) => restoreBuiltinPrice(provider, pricingId), onSuccess: finishMutation });
  const checkUpdate = useMutation({ mutationFn: previewPriceUpdate, onSuccess: setUpdatePreview });
  const applyUpdate = useMutation({
    mutationFn: (previewId: string) => applyPriceUpdate(previewId),
    onSuccess: () => { setUpdatePreview(undefined); finishMutation(); },
  });

  return (
    <section className="feature-page">
      <div className="tabs" role="tablist">
        <button type="button" role="tab" aria-selected={tab === 'usage'} className={tab === 'usage' ? 'active' : ''} onClick={() => setTab('usage')}>{t('模型用量')}</button>
        <button type="button" role="tab" aria-selected={tab === 'prices'} className={tab === 'prices' ? 'active' : ''} onClick={() => setTab('prices')}>{t('价格表')}</button>
      </div>
      {tab === 'usage' ? <>
        <PageControls
          search={search} sort={sort} descending={descending} page={page} pageSize={30}
          total={models.data?.total ?? 0}
          sortOptions={[{ value: 'tokens', label: 'Token' }, { value: 'cost', label: t('成本') }, { value: 'sessions', label: t('会话') }, { value: 'recent', label: t('最近使用') }, { value: 'name', label: t('名称') }]}
          onSearch={(value) => { setSearch(value); setPage(0); }} onSort={setSort}
          onDescending={setDescending} onPage={setPage}
        />
        <QueryStatus loading={models.isLoading && !models.data} error={models.error} onRetry={() => void models.refetch()} />
        {!models.isError && (
        <div className="data-table-wrap"><table className="data-table">
          <thead><tr><th>{t('模型')}</th><th>{t('会话')}</th><th>{t('输入')}</th><th>{t('缓存')}</th><th>{t('输出')}</th><th>{t('推理')}</th><th>{t('命中率')}</th><th>{t('成本')}</th><th>{t('平均每百万 Token 成本')}</th><th>{t('最近使用')}</th></tr></thead>
          <tbody>{models.data?.items.map((model) => <tr key={model.model}>
            <td><strong>{model.model}</strong><small>{model.pricingModelId ? `${t('计价')} ${model.pricingModelId}` : t('未匹配价格')}</small></td>
            <td>{model.sessionCount.toLocaleString()}</td><td>{formatTokens(model.freshInputTokens)}</td>
            <td>{formatTokens(model.cachedInputTokens)}</td><td>{formatTokens(model.outputTokens)}</td>
            <td>{formatTokens(model.reasoningTokens)}</td>
            <td>{model.cacheHitRate == null ? 'N/A' : `${(model.cacheHitRate * 100).toFixed(1)}%`}</td>
            <td>{formatCost(model.estimatedCostMicrousd)}{model.unpricedEventCount > 0 && <small className="warning-copy">{t('未定价')}</small>}</td>
            <td>{formatCost(model.averageCostMicrousdPerMillionTokens)}</td>
            <td>{model.lastUsedAtMs ? new Date(model.lastUsedAtMs).toLocaleString() : '—'}</td>
          </tr>)}</tbody>
        </table>{!models.isLoading && models.data?.items.length === 0 && <div className="table-empty">{t('没有匹配的模型。')}</div>}</div>
        )}
      </> : <>
        <div className="pricing-toolbar">
          <p>{t('价格单位为 USD / 1M Token。修改后只重算本地汇总，不重新读取 JSONL。')}</p>
          <div className="toolbar-actions">
            <button type="button" className="quiet-button" disabled={checkUpdate.isPending || !isTauriRuntime()} onClick={() => checkUpdate.mutate()}><CloudDownload />{checkUpdate.isPending ? t('正在检查') : t('检查官方更新')}</button>
            <button type="button" className="primary-button" disabled={!isTauriRuntime()} onClick={() => setEditing(EMPTY_PRICE)}><CircleDollarSign />{t('添加价格')}</button>
          </div>
        </div>
        <QueryStatus loading={prices.isLoading && !prices.data} error={prices.error} onRetry={() => void prices.refetch()} />
        {!prices.isError && (
        <div className="data-table-wrap"><table className="data-table">
          <thead><tr><th>{t('计价 ID')}</th><th>{t('输入')}</th><th>{t('缓存读')}</th><th>{t('缓存写')}</th><th>{t('输出')}</th><th>{t('来源')}</th><th /></tr></thead>
          <tbody>{prices.data?.map((price) => <tr key={`${price.provider}-${price.pricingId}`} className={price.isDeleted ? 'deleted-row' : ''}>
            <td><strong>{price.displayName}</strong><small>{price.provider} / {price.pricingId}</small></td>
            <td>${price.inputPerMillionUsd}</td><td>${price.cacheReadPerMillionUsd}</td>
            <td>{price.cacheWritePerMillionUsd ? `$${price.cacheWritePerMillionUsd}` : 'N/A'}</td><td>${price.outputPerMillionUsd}</td>
            <td>{price.isBuiltin ? t('内置官方快照') : t('用户价格')}{price.isOverridden && <small>{t('已覆盖')}</small>}{price.pricingId === 'gpt-5.5' && <small>{t('标准价适用于不超过 272K 输入')}</small>}</td>
            <td className="row-actions">
              {!price.isDeleted && <button type="button" className="icon-button" aria-label={t('编辑价格')} onClick={() => setEditing({
                provider: price.provider, pricingId: price.pricingId, displayName: price.displayName,
                inputPerMillionUsd: price.inputPerMillionUsd, outputPerMillionUsd: price.outputPerMillionUsd,
                cacheReadPerMillionUsd: price.cacheReadPerMillionUsd, cacheWritePerMillionUsd: price.cacheWritePerMillionUsd,
              })}><Save /></button>}
              {!price.isDeleted && <button type="button" className="icon-button danger" aria-label={t('删除价格')} onClick={() => remove.mutate(price)}><Trash2 /></button>}
              {price.isBuiltin && (price.isDeleted || price.isOverridden) && <button type="button" className="icon-button" aria-label={t('恢复内置价格')} onClick={() => restore.mutate(price)}><RotateCcw /></button>}
            </td>
          </tr>)}</tbody>
        </table>{!prices.isLoading && prices.data?.length === 0 && <div className="table-empty">{t('价格表为空。')}</div>}</div>
        )}
      </>}
      {editing && <PriceEditor value={editing} busy={save.isPending} onChange={setEditing} onCancel={() => setEditing(undefined)} onSave={() => save.mutate(editing)} />}
      {updatePreview && <section className="price-preview" aria-label={t('价格更新预览')}>
        <div><strong>{t('OpenAI 官方价格差异')}</strong><small>{new Date(updatePreview.fetchedAtMs).toLocaleString()} · {updatePreview.unchangedCount} {t('项未变化')}</small><a href={updatePreview.sourceUrl} target="_blank" rel="noreferrer">{t('查看固定可信来源')}</a></div>
        <div className="price-change-list">{updatePreview.changes.length === 0 ? <p>{t('本地内置价格已是最新。')}</p> : updatePreview.changes.map((change) => <div key={change.pricingId}><span>{change.kind === 'added' ? t('新增') : t('更新')} · {change.pricingId}</span><strong>{change.before ? `$${change.before.inputPerMillionUsd} → ` : ''}${change.after.inputPerMillionUsd} {t('输入')} / ${change.after.outputPerMillionUsd} {t('输出')}</strong></div>)}</div>
        <button type="button" className="primary-button" disabled={applyUpdate.isPending || updatePreview.changes.length === 0} onClick={() => applyUpdate.mutate(updatePreview.previewId)}>{t('确认应用并重算')}</button>
        <button type="button" className="quiet-button" onClick={() => setUpdatePreview(undefined)}>{t('取消')}</button>
      </section>}
    </section>
  );
}

function PriceEditor({ value, busy, onChange, onCancel, onSave }: {
  value: ModelPriceInput; busy: boolean; onChange: (value: ModelPriceInput) => void;
  onCancel: () => void; onSave: () => void;
}) {
  const { t } = useTranslation();
  const field = (key: keyof ModelPriceInput, label: string, required = true) => <label><span>{t(label)}</span><input required={required} value={value[key] ?? ''} onChange={(event) => onChange({ ...value, [key]: event.target.value || undefined })} /></label>;
  return <form className="inline-editor price-editor" onSubmit={(event) => { event.preventDefault(); onSave(); }}>
    <strong>{t('模型价格')}</strong>{field('provider', '提供方')}{field('pricingId', '计价 ID')}{field('displayName', '显示名称')}
    {field('inputPerMillionUsd', '输入')}{field('cacheReadPerMillionUsd', '缓存读取')}{field('cacheWritePerMillionUsd', '缓存写入（可选）', false)}{field('outputPerMillionUsd', '输出')}
    <button type="submit" className="primary-button" disabled={busy}><Save />{t('保存并重算')}</button><button type="button" className="quiet-button" onClick={onCancel}>{t('取消')}</button>
  </form>;
}
