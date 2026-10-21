import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  checkPublishedCompanionRelease,
  classifyCompanionRelease,
  releaseBuildMatrix,
} from './check-companion-release.mjs';
import {
  latestCompanionTag,
  planCompanionRelease,
} from './plan-companion-release.mjs';

const tag = 'companion-v0.1.9';
const allTargets = 'macos-arm64,macos-x64,linux-x64,linux-arm64,windows-x64';

function publishedRelease() {
  return {
    tag_name: tag,
    draft: false,
    prerelease: false,
    assets: releaseBuildMatrix('0.1.9', allTargets).map(({ target }) => ({
      name: `oh-my-opencode-slim-companion-v0.1.9-${target}.${target.includes('windows') ? 'zip' : 'tar.gz'}`,
      state: 'uploaded',
      size: 100,
    })),
  };
}

function options(request) {
  return {
    repository: 'alvinunreal/oh-my-opencode-companion',
    token: 'test-token',
    request,
    pause: async () => {},
  };
}

function response(status, data) {
  return { status, json: async () => data };
}

test('build matrix validates exact platform names, duplicates and numeric versions', () => {
  assert.equal(releaseBuildMatrix('0.1.10', allTargets).length, 5);
  assert.deepEqual(releaseBuildMatrix('0.1.10', 'windows-x64'), [
    {
      name: 'windows-x64',
      target: 'x86_64-pc-windows-msvc',
      runner: 'windows-latest',
    },
  ]);
  for (const targets of [
    '',
    undefined,
    'linux-x64,linux-x64',
    'not-macos-arm64',
    'linux-x64,',
    'linux-x64, windows-x64',
  ]) {
    assert.throws(() => releaseBuildMatrix('0.1.10', targets));
  }
  for (const version of [
    '0.01.10',
    'v0.1.10',
    '0.1.10-beta',
    '0.1.10\n',
    undefined,
  ]) {
    assert.throws(() => releaseBuildMatrix(version, allTargets));
  }
});

test('complete release requires five uploaded nonempty archives, allows extra assets', () => {
  const release = publishedRelease();
  assert.equal(classifyCompanionRelease(release, tag), 'complete');
  release.assets.push({ name: 'checksums.txt', state: 'uploaded', size: 1 });
  assert.equal(classifyCompanionRelease(release, tag), 'complete');
});

test('missing, draft, prerelease, duplicate, zero-size and pending archives are incomplete', () => {
  const mutations = [
    (r) => r.assets.pop(),
    (r) => {
      r.draft = true;
    },
    (r) => {
      r.prerelease = true;
    },
    (r) => r.assets.push(r.assets[0]),
    (r) => {
      r.assets[0].size = 0;
    },
    (r) => {
      r.assets[0].size = undefined;
    },
    (r) => {
      r.assets[0].state = 'new';
    },
  ];
  for (const mutate of mutations) {
    const release = publishedRelease();
    mutate(release);
    assert.equal(classifyCompanionRelease(release, tag), 'incomplete');
  }
});

test('malformed successful API responses fail closed', async () => {
  for (const data of [
    null,
    { assets: [] },
    { ...publishedRelease(), tag_name: 'wrong' },
  ]) {
    await assert.rejects(
      checkPublishedCompanionRelease(
        tag,
        options(async () => response(200, data)),
      ),
      /Malformed/,
    );
  }
  await assert.rejects(
    checkPublishedCompanionRelease(
      tag,
      options(async () => ({
        status: 200,
        json: async () => {
          throw new Error('bad JSON');
        },
      })),
    ),
    /Invalid.*JSON/,
  );
});

test('successful API lookup uses the standalone repository and correct tag', async () => {
  let calls = 0;
  const result = await checkPublishedCompanionRelease(
    tag,
    options(async (url) => {
      calls++;
      assert.equal(
        url,
        `https://api.github.com/repos/alvinunreal/oh-my-opencode-companion/releases/tags/${tag}`,
      );
      return response(200, publishedRelease());
    }),
  );
  assert.equal(result, 'complete');
  assert.equal(calls, 1);
});

test('transient transport and server errors retry, never manufacture missing', async () => {
  let calls = 0;
  const result = await checkPublishedCompanionRelease(
    tag,
    options(async () => {
      calls++;
      if (calls === 1) throw new Error('Connection reset');
      if (calls === 2) return response(503);
      return response(200, publishedRelease());
    }),
  );
  assert.equal(result, 'complete');
  assert.equal(calls, 3);
});

test('only persistent HTTP 404 classifies a release as missing', async () => {
  let calls = 0;
  assert.equal(
    await checkPublishedCompanionRelease(
      tag,
      options(async () => {
        calls++;
        return response(404);
      }),
    ),
    'missing',
  );
  assert.equal(calls, 3);
});

test('auth, rate limit, server and persistent transport errors fail closed', async () => {
  for (const status of [401, 403, 429, 502]) {
    let calls = 0;
    await assert.rejects(
      checkPublishedCompanionRelease(
        tag,
        options(async () => {
          calls++;
          return response(status);
        }),
      ),
      new RegExp(`HTTP ${status}`),
    );
    assert.equal(calls, [401, 403].includes(status) ? 1 : 3);
  }
  await assert.rejects(
    checkPublishedCompanionRelease(
      tag,
      options(async () => {
        throw new Error('offline');
      }),
    ),
    /failed after retries/,
  );
});

test('invalid tags and missing configuration fail before network requests', async () => {
  let calls = 0;
  const config = options(async () => {
    calls++;
  });
  for (const invalid of [undefined, 'v0.1.9', 'companion-v0.01.9']) {
    await assert.rejects(
      checkPublishedCompanionRelease(invalid, config),
      /Invalid/,
    );
  }
  await assert.rejects(
    checkPublishedCompanionRelease(tag, { ...config, token: '' }),
    /requires/,
  );
  assert.equal(calls, 0);
});

test('latest tag selection is semantic and ignores unrelated/prerelease tags', () => {
  assert.equal(
    latestCompanionTag([
      'v99.0.0',
      tag,
      'companion-v0.1.10',
      'companion-v0.1.11-beta',
    ]),
    'companion-v0.1.10',
  );
  assert.equal(
    latestCompanionTag(['companion-v1.9.100', 'companion-v2.0.0']),
    'companion-v2.0.0',
  );
  assert.throws(() => latestCompanionTag(['v1.0.0']), /Seed a release/);
});

test('unchanged complete release is a no-op; drift or incomplete state requires a NEW patch', () => {
  assert.deepEqual(planCompanionRelease(tag, false, 'complete'), {
    releaseNeeded: false,
    nextVersion: '0.1.9',
  });
  for (const state of ['complete', 'missing', 'incomplete']) {
    for (const changed of [true, false]) {
      if (state === 'complete' && !changed) continue;
      assert.deepEqual(planCompanionRelease(tag, changed, state), {
        releaseNeeded: true,
        nextVersion: '0.1.10',
      });
    }
  }
  assert.deepEqual(
    planCompanionRelease('companion-v0.1.10', false, 'missing'),
    {
      releaseNeeded: true,
      nextVersion: '0.1.11',
    },
  );
  assert.throws(
    () => planCompanionRelease(tag, false, 'unknown'),
    /Unrecognized/,
  );
  assert.throws(
    () => planCompanionRelease('invalid', false, 'complete'),
    /Invalid/,
  );
});
