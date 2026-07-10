import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { SettingsPage } from './SettingsPage';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

describe('SettingsPage', () => {
  beforeEach(async () => {
    localStorage.clear();
    document.documentElement.dataset.theme = '';
    await i18n.changeLanguage('zh-CN');
  });

  it('persists language, theme, and font scale locally', async () => {
    render(<QueryClientProvider client={new QueryClient()}><SettingsPage /></QueryClientProvider>);
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'en' } });
    await waitFor(() => expect(localStorage.getItem('chronolume.language')).toBe('en'));
    expect(screen.getByText('Theme')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Light' }));
    expect(document.documentElement.dataset.theme).toBe('light');
    fireEvent.change(screen.getByRole('slider'), { target: { value: '1.2' } });
    expect(localStorage.getItem('chronolume.font-scale')).toBe('1.2');
  });
});
