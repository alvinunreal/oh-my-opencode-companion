import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { prepareCompanionRelease } from './prepare-companion-release.mjs';
import { planCompanionRelease } from './plan-companion-release.mjs';
import { companionSourceDrift } from './verify-companion-source.mjs';

const tag = 'companion-v0.1.9';
const sourceCheck = fileURLToPath(
  new URL('./verify-companion-source.mjs', import.meta.url),
);
const prepareRelease = fileURLToPath(
  new URL('./prepare-companion-release.mjs', import.meta.url),
);

function git(cwd, ...args) {
  execFileSync('git', ['-C', cwd, ...args], { stdio: 'pipe' });
}

function fixture(t) {
  const temporaryRoot = join(tmpdir(), 'opencode');
  mkdirSync(temporaryRoot, { recursive: true });
  const root = mkdtempSync(join(temporaryRoot, 'companion-release-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'src'));
  mkdirSync(join(root, 'animations'));
  writeFileSync(
    join(root, 'Cargo.toml'),
    '[package]\nname = "oh-my-opencode-slim-companion"\nversion = "0.1.9"\n\n[dependencies]\nserde = "1"\n',
  );
  writeFileSync(
    join(root, 'Cargo.lock'),
    '[[package]]\nname = "oh-my-opencode-slim-companion"\nversion = "0.1.9"\n\n[[package]]\nname = "serde"\nversion = "1.0.0"\n',
  );
  writeFileSync(join(root, 'src/main.rs'), 'fn main() {}\n');
  writeFileSync(
    join(root, 'animations/intro.jpg'),
    Buffer.from([0xff, 0xd8, 0, 1]),
  );
  git(root, 'init', '-q');
  git(root, 'config', 'user.email', 'test@example.com');
  git(root, 'config', 'user.name', 'Companion Test');
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'Native source');
  git(root, 'tag', tag);
  return root;
}

test('root source checker permits unchanged native sources and ignores docs/media/workflows', (t) => {
  const root = fixture(t);
  assert.deepEqual(companionSourceDrift(tag, root), []);
  writeFileSync(join(root, 'README.md'), 'New docs');
  mkdirSync(join(root, 'VIDEOS'));
  writeFileSync(join(root, 'VIDEOS/source.mp4'), 'New source video');
  mkdirSync(join(root, '.github'));
  writeFileSync(join(root, '.github/release.yml'), 'workflow');
  assert.deepEqual(companionSourceDrift(tag, root), []);
});

test('committed standalone setup does not allocate a release for unchanged native source', (t) => {
  const root = fixture(t);
  for (const name of ['README.md', 'AGENTS.md', 'LICENSE', 'codemap.md']) {
    writeFileSync(join(root, name), 'Standalone project documentation\n');
  }
  writeFileSync(join(root, '.gitignore'), '/target/\n');
  mkdirSync(join(root, '.github/workflows'), { recursive: true });
  writeFileSync(
    join(root, '.github/workflows/companion-auto-release.yml'),
    'name: Native release\n',
  );
  mkdirSync(join(root, 'scripts'));
  writeFileSync(
    join(root, 'scripts/plan-companion-release.mjs'),
    '// Native release helper\n',
  );
  git(root, 'add', '.');
  git(root, 'commit', '-qm', 'Set up standalone repository');

  const drift = companionSourceDrift(tag, root);
  assert.deepEqual(drift, []);
  assert.deepEqual(planCompanionRelease(tag, drift.length > 0, 'complete'), {
    releaseNeeded: false,
    nextVersion: '0.1.9',
  });
});

test('detects Rust, binary sprite, untracked runtime, build script and dependency changes', (t) => {
  const root = fixture(t);
  writeFileSync(join(root, 'src/main.rs'), 'fn main() { println!("new"); }\n');
  writeFileSync(
    join(root, 'animations/intro.jpg'),
    Buffer.from([0xff, 0xd8, 0, 2]),
  );
  writeFileSync(join(root, 'src/new.rs'), '// new module\n');
  writeFileSync(join(root, 'build.rs'), 'fn main() {}\n');
  writeFileSync(
    join(root, 'Cargo.toml'),
    readFileSync(join(root, 'Cargo.toml'), 'utf8').replace(
      'serde = "1"',
      'serde = "2"',
    ),
  );
  assert.deepEqual(companionSourceDrift(tag, root), [
    'Cargo.toml',
    'animations/intro.jpg',
    'build.rs',
    'src/main.rs',
    'src/new.rs',
  ]);
});

test('detects runtime deletions and moves outside runtime directories', (t) => {
  const root = fixture(t);
  renameSync(join(root, 'src/main.rs'), join(root, 'example.rs'));
  rmSync(join(root, 'animations/intro.jpg'));
  assert.deepEqual(companionSourceDrift(tag, root), [
    'animations/intro.jpg',
    'src/main.rs',
  ]);
});

test('version stamping changes only the root crate versions and accepts CRLF', (t) => {
  const root = fixture(t);
  for (const name of ['Cargo.toml', 'Cargo.lock']) {
    const path = join(root, name);
    writeFileSync(path, readFileSync(path, 'utf8').replaceAll('\n', '\r\n'));
  }
  prepareCompanionRelease('0.1.10', root);
  assert.match(
    readFileSync(join(root, 'Cargo.toml'), 'utf8'),
    /version = "0.1.10"/,
  );
  const lock = readFileSync(join(root, 'Cargo.lock'), 'utf8');
  assert.match(lock, /version = "0.1.10"/);
  assert.match(lock, /name = "serde"\r\nversion = "1.0.0"/);
  assert.deepEqual(companionSourceDrift(tag, root), []);
});

test('source check does not ignore dependency versions in Cargo.lock', (t) => {
  const root = fixture(t);
  const path = join(root, 'Cargo.lock');
  writeFileSync(path, readFileSync(path, 'utf8').replace('1.0.0', '1.0.1'));
  assert.deepEqual(companionSourceDrift(tag, root), ['Cargo.lock']);
});

test('stamp validates both files before writing, rejects invalid versions and plugin-only flags', (t) => {
  const root = fixture(t);
  const cargoBefore = readFileSync(join(root, 'Cargo.toml'), 'utf8');
  for (const version of [undefined, 'v0.1.10', '0.01.10', '0.1.10-beta']) {
    assert.throws(() => prepareCompanionRelease(version, root), /numeric/);
  }
  assert.equal(
    spawnSync(
      'node',
      [prepareRelease, '--version=0.1.10', '--assets-dir=assets'],
      { cwd: root },
    ).status,
    1,
  );
  writeFileSync(join(root, 'Cargo.lock'), '# missing package\n');
  assert.throws(() => prepareCompanionRelease('0.1.10', root), /Cargo.lock/);
  assert.equal(readFileSync(join(root, 'Cargo.toml'), 'utf8'), cargoBefore);
});

test('source CLI requires explicit native tag; exit codes distinguish drift and failure', (t) => {
  const root = fixture(t);
  const run = (...args) =>
    spawnSync('node', [sourceCheck, ...args], { cwd: root, encoding: 'utf8' });
  assert.equal(run(`--tag=${tag}`).status, 0);
  assert.equal(run().status, 1);
  writeFileSync(join(root, 'src/main.rs'), '// changed\n');
  const drift = run(`--tag=${tag}`);
  assert.equal(drift.status, 2);
  assert.match(drift.stderr, /src\/main.rs/);
  git(root, 'tag', '-d', tag);
  const missing = run(`--tag=${tag}`);
  assert.equal(missing.status, 1);
  assert.match(missing.stderr, /unavailable/);
});
