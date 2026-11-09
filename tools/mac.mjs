// Start the native Tauri app; keep models, storage and credentials in Rust.
import { existsSync, readdirSync, mkdirSync, copyFileSync, writeFileSync, renameSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const env = { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '2' };

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: 'inherit', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed. Check the output above.`);
  return result;
}

try {
  if (process.platform !== 'darwin') throw new Error('The Mac app requires macOS.');
  const required = ['assets/mac/model.onnx', 'assets/mac/tokenizer.json', 'assets/mac/encoder.json',
    'assets/adapters/phone-to-mac/adapter.json', 'assets/adapters/phone-to-mac/weights.f32'];
  if (!required.every(path => existsSync(join(root, path)))) {
    run(process.execPath, ['tools/ios.mjs', '--prepare-only', '--prepare']);
  }

  if (!env.ORT_DYLIB_PATH) {
    const candidates = [env.FABRIC_PYTHON && dirname(dirname(env.FABRIC_PYTHON)), join(root, '.venv'), resolve(root, '../../work/venv')].filter(Boolean);
    function runtime() {
      for (const venv of candidates) {
        const lib = join(venv, 'lib');
        if (!existsSync(lib)) continue;
        for (const version of readdirSync(lib).filter(name => name.startsWith('python'))) {
          const capi = join(lib, version, 'site-packages/onnxruntime/capi');
          if (!existsSync(capi)) continue;
          const library = readdirSync(capi).find(name => /^libonnxruntime\..+\.dylib$/.test(name));
          if (library) return join(capi, library);
        }
      }
    }
    env.ORT_DYLIB_PATH = runtime();
    if (!env.ORT_DYLIB_PATH) {
      run(process.execPath, ['tools/ios.mjs', '--prepare-only', '--prepare']);
      env.ORT_DYLIB_PATH = runtime();
    }
    if (!env.ORT_DYLIB_PATH) throw new Error('The ONNX Runtime library is missing from the prepared environment.');
  }
  env.FABRIC_ASSET_DIR ??= join(root, 'assets');
  const dotenv = join(root, 'apps/mac/.env');
  if (!env.FABRIC_ENV_FILE && existsSync(dotenv)) env.FABRIC_ENV_FILE = dotenv;

  console.log('Building Memory Fabric for Mac…');
  run('/usr/bin/python3', ['tools/local_llm.py']);
  run('/usr/bin/python3', ['tools/web_search.py']);
  run('/usr/bin/python3', ['tools/sync/setup.py']);
  run('cargo', ['build', '--locked', '-j', '2', '-p', 'fabric-mac']);
  // A runtime bundle belongs outside iCloud-managed Documents, which may attach
  // Finder metadata during signing and invalidate an otherwise correct bundle.
  const app = join(env.FABRIC_APP_DIR ?? join(homedir(), 'Library/Application Support/local.memoryfabric.mac/App'), 'Memory Fabric.app');
  const contents = join(app, 'Contents');
  const executable = join(contents, 'MacOS/fabric-mac');
  mkdirSync(dirname(executable), { recursive: true });
  copyFileSync(join(root, 'target/debug/fabric-mac'), `${executable}.next`);
  renameSync(`${executable}.next`, executable);
  writeFileSync(join(contents, 'Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>fabric-mac</string>
<key>CFBundleIdentifier</key><string>local.memoryfabric.mac</string>
<key>CFBundleName</key><string>Memory Fabric</string>
<key>CFBundleDisplayName</key><string>Memory Fabric</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
</dict></plist>\n`);
  run('xattr', ['-cr', app]);
  run('codesign', ['--force', '--sign', '-', app]);
  const args = ['-a', app];
  for (const key of ['FABRIC_ASSET_DIR', 'ORT_DYLIB_PATH', 'FABRIC_DATA_DIR', 'FABRIC_ENV_FILE']) {
    if (env[key]) args.push('--env', `${key}=${env[key]}`);
  }
  console.log('Opening Memory Fabric.');
  run('open', args);
} catch (error) {
  console.error(`\n${error.message}`);
  process.exitCode = 1;
}
