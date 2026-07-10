import { useState } from 'react';
import { save } from '@tauri-apps/plugin-dialog';
import { Download, Image } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { exportData, isTauriRuntime, writeChartPng } from '../api';
import type { ExportFormat, ExportPrivacy, ExportScope, UsageFilters } from '../types';

interface ExportControlsProps {
  scope?: ExportScope;
  filters: UsageFilters;
  allowPng: boolean;
}

export function ExportControls({ scope, filters, allowPng }: ExportControlsProps) {
  const { t } = useTranslation();
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [privacy, setPrivacy] = useState<ExportPrivacy>('anonymous');
  const [busy, setBusy] = useState(false);

  const exportStructured = async () => {
    if (!scope || !isTauriRuntime()) return;
    const path = await save({
      defaultPath: `codex-usage-${scope}-${dateStamp()}.${format}`,
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      await exportData({ format, scope, privacy, filters }, path);
    } finally {
      setBusy(false);
    }
  };

  const exportPng = async () => {
    if (!isTauriRuntime()) return;
    const svg = document.querySelector<SVGSVGElement>('.chart-card .recharts-surface');
    if (!svg) return;
    const path = await save({
      defaultPath: `codex-usage-dashboard-${dateStamp()}.png`,
      filters: [{ name: 'PNG', extensions: ['png'] }],
    });
    if (!path) return;
    setBusy(true);
    try {
      const bytes = await svgToPng(svg);
      await writeChartPng(path, [...bytes]);
    } finally {
      setBusy(false);
    }
  };

  if (!scope) return null;
  return <div className="export-controls" aria-label={t('导出')}>
    <select aria-label={t('导出格式')} value={format} onChange={(event) => setFormat(event.target.value as ExportFormat)}>
      <option value="csv">CSV</option><option value="json">JSON</option>
    </select>
    <select aria-label={t('导出隐私')} value={privacy} onChange={(event) => setPrivacy(event.target.value as ExportPrivacy)}>
      <option value="anonymous">{t('匿名路径')}</option><option value="full_path">{t('完整路径')}</option>
    </select>
    <button type="button" className="quiet-button" disabled={busy || !isTauriRuntime()} onClick={() => void exportStructured()}><Download />{t('导出')}</button>
    {allowPng && <button type="button" className="icon-button" aria-label={t('导出趋势图 PNG')} disabled={busy || !isTauriRuntime()} onClick={() => void exportPng()}><Image /></button>}
  </div>;
}

async function svgToPng(svg: SVGSVGElement): Promise<Uint8Array> {
  const rect = svg.getBoundingClientRect();
  const width = Math.max(1, Math.round(rect.width));
  const height = Math.max(1, Math.round(rect.height));
  const clone = svg.cloneNode(true) as SVGSVGElement;
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  clone.setAttribute('width', String(width));
  clone.setAttribute('height', String(height));
  const blob = new Blob([new XMLSerializer().serializeToString(clone)], { type: 'image/svg+xml' });
  const image = new window.Image();
  const url = URL.createObjectURL(blob);
  try {
    await new Promise<void>((resolve, reject) => {
      image.onload = () => resolve();
      image.onerror = () => reject(new Error('Chart image could not be rendered'));
      image.src = url;
    });
    const canvas = document.createElement('canvas');
    canvas.width = width * 2;
    canvas.height = height * 2;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Canvas is unavailable');
    context.scale(2, 2);
    context.fillStyle = '#101219';
    context.fillRect(0, 0, width, height);
    context.drawImage(image, 0, 0, width, height);
    const png = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob((value) => value ? resolve(value) : reject(new Error('PNG encoding failed')), 'image/png'),
    );
    return new Uint8Array(await png.arrayBuffer());
  } finally {
    URL.revokeObjectURL(url);
  }
}

function dateStamp(): string {
  return new Date().toISOString().slice(0, 10);
}
