import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './App';
import { initializeTheme } from './lib/theme';
import './i18n';
import './styles.css';

initializeTheme();
const storedScale = Number(localStorage.getItem('codex-usage.font-scale') ?? 1);
document.documentElement.style.fontSize = `${Math.max(.9, Math.min(1.35, storedScale)) * 100}%`;
document.documentElement.lang = localStorage.getItem('codex-usage.language') === 'en' ? 'en' : 'zh-CN';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 15_000,
      gcTime: 5 * 60_000,
      refetchOnWindowFocus: false,
    },
  },
});

const rootElement = document.getElementById('root');
if (!rootElement) {
  throw new Error('Missing application root element');
}

createRoot(rootElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
