#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function prepareCompanionRelease(version, directory = '.') {
  if (
    version?.trim() !== version ||
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version ?? '')
  ) {
    throw new Error('Specify a numeric --version=X.Y.Z.');
  }

  const files = [
    [
      'Cargo.toml',
      /(^\[package\]\r?\n(?:(?!\[)[^\n]*\n)*?version\s*=\s*")[^"]+(".*)/m,
    ],
    [
      'Cargo.lock',
      /(\[\[package\]\]\r?\nname = "oh-my-opencode-slim-companion"\r?\nversion = ")[^"]+(")/,
    ],
  ];
  // Validate both inputs before writing either, and touch no dependency versions.
  const updates = files.map(([name, pattern]) => {
    const path = resolve(directory, name);
    const text = readFileSync(path, 'utf8');
    if (!pattern.test(text))
      throw new Error(`Could not locate crate version in ${name}`);
    return [
      path,
      text.replace(
        pattern,
        (_match, prefix, suffix) => `${prefix}${version}${suffix}`,
      ),
    ];
  });
  for (const [path, text] of updates) writeFileSync(path, text);
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    if (process.argv.slice(2).length !== 1)
      throw new Error('Specify only --version=X.Y.Z.');
    const version = process.argv[2]?.match(/^--version=(.+)$/)?.[1];
    prepareCompanionRelease(version);
    console.log(`Prepared native Companion build version ${version}`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
