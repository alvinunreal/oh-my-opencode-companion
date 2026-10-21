#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { checkPublishedCompanionRelease } from './check-companion-release.mjs';
import { companionSourceDrift } from './verify-companion-source.mjs';

const TAG_PATTERN = /^companion-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function latestCompanionTag(tags) {
  const versions = tags.filter(
    (tag) => tag.trim() === tag && TAG_PATTERN.test(tag),
  );
  versions.sort((left, right) => {
    const a = left.match(TAG_PATTERN).slice(1).map(BigInt);
    const b = right.match(TAG_PATTERN).slice(1).map(BigInt);
    for (let i = 0; i < 3; i++) {
      if (a[i] !== b[i]) return a[i] > b[i] ? -1 : 1;
    }
    return 0;
  });
  if (!versions.length)
    throw new Error(
      'No native Companion release tag found. Seed a release before enabling automation.',
    );
  return versions[0];
}

export function planCompanionRelease(tag, sourceChanged, releaseState) {
  const parts = tag.match(TAG_PATTERN);
  if (!parts || tag.trim() !== tag)
    throw new Error(`Invalid Companion release tag: ${tag}`);
  if (!['complete', 'missing', 'incomplete'].includes(releaseState)) {
    throw new Error(`Unrecognized Companion release state: ${releaseState}`);
  }
  const releaseNeeded = sourceChanged || releaseState !== 'complete';
  const version = parts.slice(1).join('.');
  return {
    releaseNeeded,
    nextVersion: releaseNeeded
      ? `${parts[1]}.${parts[2]}.${BigInt(parts[3]) + 1n}`
      : version,
  };
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    // Fetch errors are fatal; stale tag lists must never choose a release version.
    execFileSync('git', ['fetch', '--tags', 'origin'], { stdio: 'inherit' });
    const tags = execFileSync('git', ['tag', '-l', 'companion-v*'], {
      encoding: 'utf8',
    })
      .trim()
      .split('\n');
    const tag = latestCompanionTag(tags);
    const drift = companionSourceDrift(tag);
    const state = await checkPublishedCompanionRelease(tag);
    const plan = planCompanionRelease(tag, drift.length > 0, state);
    const output = `release_needed=${plan.releaseNeeded}\nnext_version=${plan.nextVersion}\n`;
    if (process.env.GITHUB_OUTPUT)
      appendFileSync(process.env.GITHUB_OUTPUT, output);
    console.log(
      `${tag}: ${state}; native drift: ${drift.join(', ') || 'none'}\n${output}`,
    );
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
