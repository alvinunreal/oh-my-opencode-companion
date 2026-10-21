import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  latestCompanionTag,
  planCompanionRelease,
} from './plan-companion-release.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const release = readFileSync(
  join(root, '.github/workflows/companion-release.yml'),
  'utf8',
);
const automatic = readFileSync(
  join(root, '.github/workflows/companion-auto-release.yml'),
  'utf8',
);
const prepare = release.split('\n  prepare:\n')[1].split('\n  build:\n')[0];
const build = release.split('\n  build:\n')[1].split('\n  release:\n')[0];
const publish = release.split('\n  release:\n')[1];
const allTargets = 'macos-arm64,macos-x64,linux-x64,linux-arm64,windows-x64';

test('automatic caller requests planning without precomputed versions or an outer lock', () => {
  assert.match(
    automatic,
    /uses: \.\/\.github\/workflows\/companion-release\.yml/,
  );
  assert.match(automatic, /automatic: true/);
  assert.doesNotMatch(
    automatic,
    /detect:|version:|plan-companion-release|concurrency:|runs-on:/,
  );
});

test('planning and source resolution are inside the same publication lock as manual releases', () => {
  assert.match(
    release,
    /^concurrency:\n  group: companion-native-publish\n  cancel-in-progress: false/m,
  );
  assert.match(prepare, /id: plan\n        if: inputs\.automatic/);
  assert.match(prepare, /run: node scripts\/plan-companion-release\.mjs/);
  assert.doesNotMatch(build + publish, /plan-companion-release/);
  assert.match(
    release,
    /version:\n        required: false\n        default: ''/,
  );
  assert.match(
    release,
    /automatic:[\s\S]*?default: false\n        type: boolean/,
  );
  assert.match(
    release,
    /workflow_dispatch:[\s\S]*?version:[\s\S]*?required: true/,
  );
});

test('queued automatic runs resolve current default branch and pin that SHA/version across all jobs', () => {
  assert.match(
    prepare,
    /ref: \$\{\{ inputs\.automatic && github\.event\.repository\.default_branch \|\| github\.sha \}\}/,
  );
  assert.match(prepare, /fetch-depth: 0/);
  assert.match(prepare, /source_revision=\$\(git rev-parse HEAD\)/);
  assert.ok(prepare.indexOf('actions/checkout@') < prepare.indexOf('id: plan'));
  assert.match(
    prepare,
    /VERSION: \$\{\{ steps\.plan\.outputs\.next_version \|\| inputs\.version \}\}/,
  );
  for (const job of [build, publish]) {
    assert.match(
      job,
      /ref: \$\{\{ needs\.prepare\.outputs\.source_revision \}\}/,
    );
    assert.match(job, /VERSION: \$\{\{ needs\.prepare\.outputs\.version \}\}/);
    assert.doesNotMatch(
      job,
      /inputs\.version|github\.sha|repository\.default_branch/,
    );
  }
  assert.match(build, /if: needs\.prepare\.outputs\.build == 'true'/);
  assert.match(publish, /if: needs\.prepare\.outputs\.publish == 'true'/);
});

function runSelection(t, env) {
  const temporaryRoot = join(tmpdir(), 'opencode');
  mkdirSync(temporaryRoot, { recursive: true });
  const directory = mkdtempSync(join(temporaryRoot, 'companion-workflow-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const outputPath = join(directory, 'output');
  const script = prepare
    .match(/node --input-type=module <<'NODE'\n([\s\S]*?)\n          NODE/)[1]
    .split('\n')
    .map((line) => line.slice(10))
    .join('\n');
  const result = spawnSync('node', ['--input-type=module'], {
    cwd: root,
    input: script,
    encoding: 'utf8',
    env: {
      ...process.env,
      VERSION: '0.1.9',
      AUTOMATIC: 'true',
      RELEASE_NEEDED: 'false',
      TARGETS: allTargets,
      GITHUB_OUTPUT: outputPath,
      ...env,
    },
  });
  return {
    result,
    output:
      result.status === 0
        ? Object.fromEntries(
            readFileSync(outputPath, 'utf8')
              .trim()
              .split('\n')
              .map((line) => {
                const separator = line.indexOf('=');
                return [line.slice(0, separator), line.slice(separator + 1)];
              }),
          )
        : undefined,
  };
}

test('initial unchanged release emits no build/publication; next resolved version threads through selection', (t) => {
  assert.match(
    prepare,
    /RELEASE_NEEDED: \$\{\{ !inputs\.automatic \|\| steps\.plan\.outputs\.release_needed == 'true' \}\}/,
  );
  const noop = runSelection(t, {});
  assert.equal(noop.result.status, 0, noop.result.stderr);
  assert.equal(noop.output.build, 'false');
  assert.equal(noop.output.publish, 'false');
  assert.equal(noop.output.version, '0.1.9');
  const next = runSelection(t, { VERSION: '0.1.10', RELEASE_NEEDED: 'true' });
  assert.equal(next.result.status, 0, next.result.stderr);
  assert.equal(next.output.build, 'true');
  assert.equal(next.output.publish, 'true');
  assert.equal(next.output.version, '0.1.10');
  assert.equal(JSON.parse(next.output.matrix).include.length, 5);
});

test('manual partial builds remain artifacts-only; automatic partial releases are rejected', (t) => {
  const manual = runSelection(t, {
    AUTOMATIC: 'false',
    RELEASE_NEEDED: 'true',
    TARGETS: 'linux-x64',
  });
  assert.equal(manual.result.status, 0, manual.result.stderr);
  assert.equal(manual.output.build, 'true');
  assert.equal(manual.output.publish, 'false');
  const partialAuto = runSelection(t, {
    RELEASE_NEEDED: 'true',
    TARGETS: 'linux-x64',
  });
  assert.notEqual(partialAuto.result.status, 0);
  assert.match(partialAuto.result.stderr, /all five platforms/);
});

test('post-lock planning observes finished manual upload instead of publishing a needless successor', () => {
  const tagsAfterManual = ['companion-v0.1.9', 'companion-v0.1.10'];
  const tag = latestCompanionTag(tagsAfterManual);
  // Before the manual upload finished, the same tag would look missing. The
  // workflow contract above prevents observing that intermediate state.
  assert.equal(
    planCompanionRelease(tag, false, 'missing').nextVersion,
    '0.1.11',
  );
  assert.deepEqual(planCompanionRelease(tag, false, 'complete'), {
    releaseNeeded: false,
    nextVersion: '0.1.10',
  });
});
