import { Search, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { WorkspaceCatalogItem } from '../types';

interface WorkspaceVisibilityDialogProps {
  options: WorkspaceCatalogItem[];
  selectedIds: string[];
  saving: boolean;
  onClose: () => void;
  onSave: (ids: string[]) => void;
}

export function WorkspaceVisibilityDialog({
  options,
  selectedIds,
  saving,
  onClose,
  onSave,
}: WorkspaceVisibilityDialogProps) {
  const { t } = useTranslation();
  const [search, setSearch] = useState('');
  const [draft, setDraft] = useState(() => new Set(selectedIds));
  const visibleOptions = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase();
    return needle
      ? options.filter((option) => `${option.label}\n${option.normalizedPath}`.toLocaleLowerCase().includes(needle))
      : options;
  }, [options, search]);
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  const toggle = (id: string, checked: boolean) => {
    setDraft((current) => {
      const next = new Set(current);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <section className="workspace-dialog" role="dialog" aria-modal="true" aria-labelledby="workspace-dialog-title">
        <header>
          <div>
            <h2 id="workspace-dialog-title">{t('选择显示的工作区')}</h2>
            <p>{t('勾选后会显示在项目页和全局工作区快捷列表中。')}</p>
          </div>
          <button type="button" className="icon-button" aria-label={t('关闭')} onClick={onClose}><X /></button>
        </header>
        <label className="workspace-search">
          <Search aria-hidden="true" />
          <span className="sr-only">{t('搜索')}</span>
          <input value={search} placeholder={t('搜索工作区…')} onChange={(event) => setSearch(event.target.value)} autoFocus />
        </label>
        <div className="workspace-dialog-actions">
          <span>{t('已选择 {{count}} 个', { count: draft.size })}</span>
          <button type="button" onClick={() => setDraft(new Set(options.map((option) => option.value)))}>{t('全选')}</button>
          <button type="button" onClick={() => setDraft(new Set())}>{t('全部取消')}</button>
        </div>
        <div className="workspace-option-list">
          {visibleOptions.map((option) => (
            <label key={option.value}>
              <input type="checkbox" checked={draft.has(option.value)} onChange={(event) => toggle(option.value, event.target.checked)} />
              <span><strong>{option.label}</strong><small>{option.normalizedPath}</small></span>
            </label>
          ))}
          {visibleOptions.length === 0 && <p>{t('没有匹配的工作区。')}</p>}
        </div>
        <footer>
          <button type="button" className="quiet-button" onClick={onClose}>{t('取消')}</button>
          <button type="button" className="primary-button" disabled={saving} onClick={() => onSave([...draft])}>{t('保存显示范围')}</button>
        </footer>
      </section>
    </div>
  );
}
