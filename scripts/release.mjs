import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8' }).trim();
const json = async (path) => JSON.parse(await readFile(resolve(root, path), 'utf8'));
const save = (path, value) => writeFile(resolve(root, path), JSON.stringify(value, null, 2) + '\n');
const metadataPath = 'target/release-metadata.json';

export function requireReleaseTag(version, event, ref) {
  assert.match(version, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/);
  assert.equal(event, 'push');
  assert.equal(ref, `refs/tags/v${version}`);
}

async function version() {
  const cargo = await readFile(resolve(root, 'Cargo.toml'), 'utf8');
  const version = cargo.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
  assert.match(version, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/);
  assert.equal((await json('goodtype-addon.json')).version, version);
  return version;
}

async function policy() {
  const releaseVersion = await version();
  if (process.env.GITHUB_REF?.startsWith('refs/tags/')) {
    requireReleaseTag(releaseVersion, 'push', process.env.GITHUB_REF);
    git('merge-base', '--is-ancestor', 'HEAD', 'origin/main');
    assert.equal(git('rev-parse', 'HEAD'), process.env.GITHUB_SHA);
  }
}

async function prepare() {
  const releaseVersion = await version();
  const metadata = {
    version: releaseVersion, sourceCommit: git('rev-parse', 'HEAD'),
    sourceDirty: git('status', '--porcelain').length > 0, target: 'x86_64-pc-windows-msvc',
    packageName: `goodtype-math-recognizer-${releaseVersion}-windows-x64`, createdAt: new Date().toISOString(),
    buildUrl: process.env.GITHUB_ACTIONS
      ? `${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}` : null,
  };
  await mkdir(resolve(root, 'target'), { recursive: true });
  await save(metadataPath, metadata);
  console.log(JSON.stringify(metadata));
}

export async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

async function publish(directory) {
  const { readdir } = await import('node:fs/promises');
  const names = await readdir(directory);
  const manifests = names.filter((name) => /^goodtype-math-recognizer-.*-windows-x64\.json$/.test(name));
  assert.equal(manifests.length, 1);
  const metadata = JSON.parse(await readFile(resolve(directory, manifests[0]), 'utf8'));
  assert.equal(metadata.sourceCommit, process.env.GITHUB_SHA);
  assert.equal(metadata.sourceDirty, false);
  assert.equal(basename(metadata.archive.fileName), metadata.archive.fileName);
  assert.equal(await sha256(resolve(directory, metadata.archive.fileName)), metadata.archive.sha256);
  assert.equal(metadata.version, await version());
  requireReleaseTag(metadata.version, process.env.GITHUB_EVENT_NAME, process.env.GITHUB_REF);
  assert.deepEqual([...names].sort(), [metadata.archive.fileName, `${metadata.packageName}.json`, 'SHA256SUMS', 'recognizer-update.json', 'recognizer-update.json.sig', 'RELEASE_NOTES.md'].sort());
  const tag = `v${metadata.version}`;
  const repository = process.env.GITHUB_REPOSITORY;
  gh('release', 'create', tag, ...names.map(name => resolve(directory, name)),
    '--repo', repository, '--verify-tag', '--title', `Goodtype Math Recognition ${tag}`,
    '--notes-file', resolve(directory, 'RELEASE_NOTES.md'));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, argument] = process.argv.slice(2);
  try {
    if (command === 'prepare') await prepare();
    else if (command === 'policy') await policy();
    else if (command === 'publish') await publish(resolve(argument));
    else throw new Error('Usage: node scripts/release.mjs prepare | policy | publish DIRECTORY');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
