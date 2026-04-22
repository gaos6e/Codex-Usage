import { test } from '@playwright/test';

test.skip(!process.env.RUN_E2E, 'Set RUN_E2E=1 after npm run package to launch packaged Electron manually.');

test('placeholder for packaged Electron smoke flow', async () => {
  // The default CI-safe command intentionally skips this test unless RUN_E2E=1.
  // Manual acceptance: launch the packaged app, verify Dashboard, Settings,
  // Project Detail, Diagnostics, export, and theme switching.
});
