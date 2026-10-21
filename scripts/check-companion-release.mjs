#!/usr/bin/env node
import { resolve } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { pathToFileURL } from 'node:url';

const PLATFORMS = [
  {
    name: 'macos-arm64',
    target: 'aarch64-apple-darwin',
    runner: 'macos-latest',
  },
  { name: 'macos-x64', target: 'x86_64-apple-darwin', runner: 'macos-latest' },
  {
    name: 'linux-x64',
    target: 'x86_64-unknown-linux-gnu',
    runner: 'ubuntu-latest',
  },
  {
    name: 'linux-arm64',
    target: 'aarch64-unknown-linux-gnu',
    runner: 'ubuntu-24.04-arm',
  },
  {
    name: 'windows-x64',
    target: 'x86_64-pc-windows-msvc',
    runner: 'windows-latest',
  },
];
const MAX_ATTEMPTS = 3;

export function releaseBuildMatrix(version, targets) {
  if (
    version?.trim() !== version ||
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version ?? '')
  ) {
    throw new Error(`Invalid Companion release version: ${version}`);
  }
  const names = targets?.split(',') ?? [];
  if (
    !names.length ||
    new Set(names).size !== names.length ||
    names.some((name) => !PLATFORMS.some((platform) => platform.name === name))
  ) {
    throw new Error(`Invalid Companion build targets: ${targets}`);
  }
  return PLATFORMS.filter((platform) => names.includes(platform.name));
}

export function classifyCompanionRelease(release, tag) {
  if (
    !release ||
    typeof release !== 'object' ||
    typeof release.draft !== 'boolean' ||
    typeof release.prerelease !== 'boolean' ||
    release.tag_name !== tag ||
    !Array.isArray(release.assets)
  ) {
    throw new Error('Malformed GitHub Companion release response.');
  }

  if (release.draft || release.prerelease) return 'incomplete';

  const version = tag.slice('companion-v'.length);
  const assets = new Map();
  for (const asset of release.assets) {
    if (typeof asset?.name !== 'string') {
      throw new Error('Malformed GitHub Companion release asset.');
    }
    if (assets.has(asset.name)) return 'incomplete';
    assets.set(asset.name, asset);
  }

  for (const { target } of PLATFORMS) {
    const extension = target.includes('windows') ? 'zip' : 'tar.gz';
    const name = `oh-my-opencode-slim-companion-v${version}-${target}.${extension}`;
    const asset = assets.get(name);
    if (
      asset?.state !== 'uploaded' ||
      !Number.isSafeInteger(asset?.size) ||
      asset.size <= 0
    ) {
      return 'incomplete';
    }
  }

  return 'complete';
}

export async function checkPublishedCompanionRelease(tag, options = {}) {
  if (
    tag?.trim() !== tag ||
    !/^companion-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? '')
  ) {
    throw new Error(`Invalid Companion release tag: ${tag}`);
  }

  const repository = options.repository ?? process.env.GITHUB_REPOSITORY;
  const token = options.token ?? process.env.GH_TOKEN;
  if (!repository || !/^[\w.-]+\/[\w.-]+$/.test(repository) || !token) {
    throw new Error(
      'Companion lookup requires GITHUB_REPOSITORY and GH_TOKEN.',
    );
  }

  const request = options.request ?? fetch;
  const pause = options.pause ?? sleep;
  const apiBase = process.env.GITHUB_API_URL ?? 'https://api.github.com';
  const url = `${apiBase}/repos/${repository}/releases/tags/${tag}`;
  const headers = {
    Accept: 'application/vnd.github+json',
    Authorization: `Bearer ${token}`,
    'X-GitHub-Api-Version': '2022-11-28',
  };

  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
    let response;
    try {
      response = await request(url, {
        headers,
        signal: AbortSignal.timeout(15_000),
      });
    } catch (error) {
      if (attempt === MAX_ATTEMPTS) {
        throw new Error(
          'GitHub Companion release lookup failed after retries.',
          {
            cause: error,
          },
        );
      }
      await pause(attempt * 1_000);
      continue;
    }

    if (response.status === 200) {
      let release;
      try {
        release = await response.json();
      } catch (error) {
        throw new Error('Invalid GitHub Companion release JSON.', {
          cause: error,
        });
      }
      return classifyCompanionRelease(release, tag);
    }

    if (response.status === 404) {
      if (attempt === MAX_ATTEMPTS) return 'missing';
      await pause(attempt * 1_000);
      continue;
    }

    // GitHub auth/permission failures, rate limits and server errors must
    // NEVER turn into "missing": that would publish an unnecessary new binary.
    if (
      attempt === MAX_ATTEMPTS ||
      ![408, 429, 500, 502, 503, 504].includes(response.status)
    ) {
      throw new Error(
        `GitHub Companion release lookup returned HTTP ${response.status}.`,
      );
    }
    await pause(attempt * 1_000);
  }

  throw new Error('Unable to determine published Companion release state.');
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  const tag = process.argv.find((arg) => arg.startsWith('--tag='))?.slice(6);
  checkPublishedCompanionRelease(tag)
    .then((state) => console.log(state))
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
