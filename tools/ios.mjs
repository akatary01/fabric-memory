// One entry point: prepare a fresh checkout if needed, then open Xcode.
import { existsSync, readFileSync, writeFileSync, copyFileSync, mkdirSync, constants } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const project = join(root, 'apps/iphone/FabricPhone.xcodeproj');
const bundle = join(root, 'apps/iphone/DemoAssets');

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', shell: false });
  if (result.error) throw new Error(`Could not run ${command}: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`${command} failed (${result.signal ?? result.status}).`);
}

function available(command, args = ['--version']) {
  return spawnSync(command, args, { cwd: root, stdio: 'ignore' }).status === 0;
}

function prepared() {
  const files = ['demo.json', 'job.json', 'incoming.jsonl', 'model/model.onnx',
    'model/tokenizer.json', 'model/encoder.json', 'adapter/weights.f32', 'adapter/adapter.json'];
  if (!files.every(file => existsSync(join(bundle, file)))) return false;
  try {
    const { demo_id: id } = JSON.parse(readFileSync(join(bundle, 'demo.json'), 'utf8'));
    return /^[a-f0-9]{64}$/.test(id) && existsSync(join(root, 'assets/testflight', id, 'demo.json'));
  } catch { return false; }
}

function repairPreparedAssets() {
  // iCloud does not retain all symlinks. Restore missing staged resources from
  // their canonical local assets, without repeating embedding or adapter training.
  function copy(source, destination, expectedHash) {
    if (existsSync(destination) || !existsSync(source)) return;
    if (expectedHash && createHash('sha256').update(readFileSync(source)).digest('hex') !== expectedHash) return;
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(source, destination, constants.COPYFILE_FICLONE);
  }
  for (const name of ['model.onnx', 'encoder.json', 'tokenizer.json', 'LICENSE', 'README.md']) {
    copy(join(root, 'assets/phone', name), join(bundle, 'model', name));
  }
  for (const name of ['adapter.json', 'weights.f32']) {
    copy(join(root, 'assets/adapters/mac-to-phone', name), join(bundle, 'adapter', name));
  }
  const metadata = join(bundle, 'demo.json');
  if (!existsSync(metadata)) return;
  const demo = JSON.parse(readFileSync(metadata, 'utf8'));
  if (!/^[a-f0-9]{64}$/.test(demo.demo_id)) return;
  copy(join(root, 'assets/job.json'), join(bundle, 'job.json'), demo.job_sha256);
  copy(join(root, 'assets/testflight', demo.demo_id, 'mac-events.jsonl'), join(bundle, 'incoming.jsonl'), demo.mac_events_sha256);
}

try {
  if (process.platform !== 'darwin') throw new Error('The iOS project requires macOS and Xcode.');
  if (!available('xcrun', ['--find', 'xcodebuild'])) throw new Error('Install Xcode and select it with xcode-select first.');

  repairPreparedAssets();
  if (!prepared() || process.argv.includes('--prepare')) {
    console.log('First-time setup: downloading models and preparing the translation demo. This takes a few minutes.');
    if (!available('cargo')) throw new Error('Install the Rust toolchain (rustup.rs), then run npm run ios again.');
    const uv = available('uv');
    const python = join(root, '.venv/bin/python');
    if (!existsSync(python)) {
      if (uv) run('uv', ['venv', '.venv', '--python', '3.12', '--allow-existing']);
      else {
        if (!available('python3', ['-c', 'import sys; sys.exit(0 if sys.version_info >= (3,12) else 1)'])) {
          throw new Error('Install Python 3.12+ or uv, then run npm run ios again.');
        }
        run('python3', ['-m', 'venv', '.venv']);
      }
    }
    const lock = join(root, 'tools/requirements.lock');
    const stamp = join(root, '.venv/.fabric-requirements');
    const hash = createHash('sha256').update(readFileSync(lock)).digest('hex');
    if (!existsSync(stamp) || readFileSync(stamp, 'utf8') !== hash) {
      if (uv) run('uv', ['pip', 'install', '--python', python, '-r', lock]);
      else run(python, ['-m', 'pip', 'install', '-r', lock]);
      writeFileSync(stamp, hash);
    }
    run(python, ['tools/prepare.py', '--anchors', '2000']);
    run(python, ['tools/train.py']);
    run('cargo', ['build', '--release', '--locked', '-j', '2', '-p', 'fabric-core']);
    run(python, ['tools/stage_testflight.py', '--skip-stress']);
    if (!prepared()) throw new Error('Demo preparation did not finish. Check the output above and retry.');
  }

  // Preserve signing and other settings edited by the user in Xcode.
  run('/usr/bin/python3', ['tools/stage_phone_generation.py']);
  if (!existsSync(join(project, 'project.pbxproj'))) run('python3', ['tools/generate_xcode.py']);
  if (!process.argv.includes('--prepare-only')) {
    console.log('Opening Memory Fabric in Xcode.');
    run('open', ['-a', 'Xcode', project]);
  }
} catch (error) {
  console.error(`\n${error.message}`);
  process.exitCode = 1;
}
