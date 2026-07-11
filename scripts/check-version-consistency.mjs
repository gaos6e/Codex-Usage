import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const read = (path) => readFileSync(resolve(root, path), 'utf8');
const json = (path) => JSON.parse(read(path));
const fail = (message) => {
  throw new Error(message);
};
const expectEqual = (label, actual, expected) => {
  if (actual !== expected) fail(`${label}: expected ${expected}, received ${actual}`);
};
const expectMatch = (label, text, pattern) => {
  if (!pattern.test(text)) fail(`${label}: ${pattern} did not match`);
};

const packageJson = json('package.json');
const packageLock = json('package-lock.json');
const tauriConfig = json('src-tauri/tauri.conf.json');
const windowsConfig = json('src-tauri/tauri.windows.conf.json');
const macosConfig = json('src-tauri/tauri.macos.conf.json');
const version = packageJson.version;
const escapedVersion = version.replaceAll('.', '\\.');
const lockedTauriSchema = '../node_modules/@tauri-apps/cli/config.schema.json';

expectEqual('package-lock top-level version', packageLock.version, version);
expectEqual('package-lock root package version', packageLock.packages[''].version, version);
expectEqual('Tauri common config version', tauriConfig.version, version);
expectEqual('Tauri CLI version is directly pinned', packageJson.devDependencies['@tauri-apps/cli'], '2.11.4');
expectEqual(
  'package-lock root Tauri CLI version is directly pinned',
  packageLock.packages[''].devDependencies['@tauri-apps/cli'],
  '2.11.4',
);
expectEqual('package-lock installed Tauri CLI version', packageLock.packages['node_modules/@tauri-apps/cli'].version, '2.11.4');
expectEqual('Tauri common schema', tauriConfig.$schema, lockedTauriSchema);
expectEqual('Tauri Windows schema', windowsConfig.$schema, lockedTauriSchema);
expectEqual('Tauri macOS schema', macosConfig.$schema, lockedTauriSchema);
expectMatch('Cargo.toml package version', read('src-tauri/Cargo.toml'), new RegExp(`^version = "${escapedVersion}"$`, 'm'));
expectMatch(
  'Cargo.lock Chronolume package version',
  read('src-tauri/Cargo.lock'),
  new RegExp(`\\[\\[package\\]\\]\\r?\\nname = "chronolume"\\r?\\nversion = "${escapedVersion}"`),
);
expectMatch('App footer fallback', read('ui/src/App.tsx'), new RegExp(`Chronolume \\{bootstrap\\.data\\?\\.appVersion \\?\\? '${escapedVersion}'\\}`));
expectMatch('browser fallback', read('ui/src/api.ts'), new RegExp(`appVersion: '${escapedVersion}-dev'`));
expectMatch('settings version', read('ui/src/features/settings/SettingsPage.tsx'), new RegExp(`Chronolume ${escapedVersion}`));
expectMatch('CHANGELOG release heading', read('CHANGELOG.md'), new RegExp(`^## ${escapedVersion}\\b`, 'm'));

expectEqual('Windows bundle identifier', windowsConfig.identifier, 'com.gaos6e.codexusage');
expectEqual('Windows bundle target', JSON.stringify(windowsConfig.bundle.targets), JSON.stringify(['nsis']));
expectEqual('macOS bundle identifier', macosConfig.identifier, 'com.gaos6e.chronolume');
expectEqual('macOS minimum version', macosConfig.bundle.macOS.minimumSystemVersion, '12.0');
expectEqual('macOS hardened runtime', macosConfig.bundle.macOS.hardenedRuntime, true);
expectEqual('macOS bundle targets', JSON.stringify(macosConfig.bundle.targets), JSON.stringify(['app', 'dmg']));

console.log(`Chronolume version and platform configuration are consistent at ${version}.`);
