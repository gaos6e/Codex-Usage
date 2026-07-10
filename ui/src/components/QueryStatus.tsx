import { useTranslation } from 'react-i18next';

export function QueryStatus({
  loading,
  error,
  onRetry,
}: {
  loading: boolean;
  error: unknown;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  if (loading) {
    return <div className="inline-loading" role="status">{t('正在加载页面数据…')}</div>;
  }
  if (!error) return null;
  return (
    <section className="state-card error-state" role="alert">
      <strong>{t('无法读取页面数据')}</strong>
      <p>{error instanceof Error ? error.message : t('发生未知错误。')}</p>
      <button type="button" onClick={onRetry}>{t('重试查询')}</button>
    </section>
  );
}
