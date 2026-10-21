#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

function releaseRelevant(path) {
  return (
    path.startsWith('src/') ||
    path.startsWith('assets/') ||
    path.startsWith('animations/') ||
    path === 'build.rs' ||
    path === 'Cargo.toml' ||
    path === 'Cargo.lock'
  );
}

function withoutCrateVersion(path, text) {
  const normalized = text.replace(/\r\n/g, '\n');
  if (path === 'Cargo.toml') {
    return normalized.replace(
      /(^\[package\]\n(?:(?!\[)[^\n]*\n)*?version\s*=\s*")[^"]+(")/m,
      '$1<version>$2',
    );
  }
  if (path === 'Cargo.lock') {
    return normalized.replace(
      /(\[\[package\]\]\nname = "oh-my-opencode-slim-companion"\nversion = ")[^"]+(")/,
      '$1<version>$2',
    );
  }
  return normalized;
}

export function companionSourceDrift(tag, directory = '.') {
  if (
    tag?.trim() !== tag ||
    !/^companion-v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag ?? '')
  ) {
    throw new Error(`Invalid Companion release tag: ${tag}`);
  }
  const git = (...args) =>
    execFileSync('git', args, {
      cwd: directory,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  try {
    git('rev-parse', '--verify', `refs/tags/${tag}^{commit}`);
  } catch {
    throw new Error(
      `Companion release tag ${tag} is unavailable. Fetch release tags first (git fetch --tags origin).`,
    );
  }

  const changed = [
    ...new Set([
      ...git('diff', '--no-renames', '--name-only', '-z', tag, '--', '.').split(
        '\0',
      ),
      ...git(
        'ls-files',
        '--others',
        '--exclude-standard',
        '-z',
        '--',
        '.',
      ).split('\0'),
    ]),
  ]
    .filter(releaseRelevant)
    .sort();

  return changed.filter((path) => {
    if (path !== 'Cargo.toml' && path !== 'Cargo.lock') return true;
    const currentPath = resolve(directory, path);
    try {
      const released = git('show', `${tag}:${path}`);
      const current = existsSync(currentPath)
        ? readFileSync(currentPath, 'utf8')
        : '';
      return (
        withoutCrateVersion(path, released) !==
        withoutCrateVersion(path, current)
      );
    } catch {
      return true;
    }
  });
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    const tag = process.argv.find((arg) => arg.startsWith('--tag='))?.slice(6);
    const drift = companionSourceDrift(tag);
    if (drift.length) {
      console.error(
        `Companion native sources have changed since ${tag}:\n- ${drift.join('\n- ')}\nPublish a new immutable native Companion release.`,
      );
      process.exitCode = 2;
    } else {
      console.log(`Companion native sources match release ${tag}.`);
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
