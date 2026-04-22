import fs from 'fs/promises';
import path from 'path';
import type { ForgeConfig } from '@electron-forge/shared-types';
import { MakerSquirrel } from '@electron-forge/maker-squirrel';
import { MakerZIP } from '@electron-forge/maker-zip';
import { MakerDeb } from '@electron-forge/maker-deb';
import { MakerRpm } from '@electron-forge/maker-rpm';
import { AutoUnpackNativesPlugin } from '@electron-forge/plugin-auto-unpack-natives';
import { WebpackPlugin } from '@electron-forge/plugin-webpack';
import { FusesPlugin } from '@electron-forge/plugin-fuses';
import { FuseV1Options, FuseVersion } from '@electron/fuses';

import { mainConfig } from './webpack.main.config';
import { rendererConfig } from './webpack.renderer.config';

const config: ForgeConfig = {
  packagerConfig: {
    asar: true,
    name: 'CodexUsage',
    executableName: 'CodexUsage',
  },
  hooks: {
    postPackage: async (_forgeConfig, packageResult) => {
      const keepLocales = new Set(['en-US.pak', 'zh-CN.pak']);
      await Promise.all(
        packageResult.outputPaths.map(async (outputPath) => {
          const localesDir = path.join(outputPath, 'locales');
          try {
            const entries = await fs.readdir(localesDir, { withFileTypes: true });
            await Promise.all(
              entries
                .filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.pak') && !keepLocales.has(entry.name))
                .map((entry) => fs.rm(path.join(localesDir, entry.name), { force: true })),
            );
          } catch {
            // Ignore missing locales directories so packaging still succeeds on unexpected layouts.
          }
        }),
      );
    },
  },
  rebuildConfig: {},
  makers: [
    new MakerSquirrel({
      name: 'CodexUsage',
      setupExe: 'Codex Usage Setup.exe',
    }),
    new MakerZIP({}, ['darwin']),
    new MakerRpm({}),
    new MakerDeb({}),
  ],
  plugins: [
    new AutoUnpackNativesPlugin({}),
    new WebpackPlugin({
      mainConfig,
      renderer: {
        config: rendererConfig,
        entryPoints: [
          {
            html: './src/renderer/index.html',
            js: './src/renderer/index.tsx',
            name: 'main_window',
            preload: {
              js: './src/preload/index.ts',
            },
          },
        ],
      },
    }),
    // Fuses are used to enable/disable various Electron functionality
    // at package time, before code signing the application
    new FusesPlugin({
      version: FuseVersion.V1,
      [FuseV1Options.RunAsNode]: false,
      [FuseV1Options.EnableCookieEncryption]: true,
      [FuseV1Options.EnableNodeOptionsEnvironmentVariable]: false,
      [FuseV1Options.EnableNodeCliInspectArguments]: false,
      [FuseV1Options.EnableEmbeddedAsarIntegrityValidation]: true,
      [FuseV1Options.OnlyLoadAppFromAsar]: true,
    }),
  ],
};

export default config;
