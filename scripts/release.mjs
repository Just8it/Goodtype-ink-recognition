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

export function buildVersion(base, channel, run, attempt, timestamp) {
  assert.match(base, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/,
    'The shared base version must be a stable major.minor.patch version.');
  if (channel === 'stable') return base;
  if (channel === 'local') {
    assert.match(timestamp, /^\d{14}$/);
    return `${base}-local.${timestamp}`;
  }
  assert.equal(channel, 'dev');
  assert.match(run ?? '', /^[1-9]\d*$/);
  assert.match(attempt ?? '', /^[1-9]\d*$/);
  return `${base}-dev.${run}.${attempt}`;
}

export function promotion(body, baseVersion) {
  body = body.replace(/<!--[\s\S]*?-->/g, '');
  const tag = body.match(/^Tested dev build: (v\d+\.\d+\.\d+-dev\.[1-9]\d*\.[1-9]\d*)\s*$/m)?.[1];
  assert.ok(tag?.startsWith(`v${baseVersion}-dev.`), 'Name the matching candidate: Tested dev build: vX.Y.Z-dev.RUN.ATTEMPT');
  assert.match(body, /^Test evidence: \S[^\r\n]*$/m, 'Add Test evidence: followed by the actual tests and results.');
  return tag;
}

export function checkPromotionBranch(pr, repository) {
  assert.equal(pr.head.repo.full_name, repository, 'Promote this repository dev branch.');
  assert.equal(pr.head.ref, 'dev', 'Only dev may be merged into main. Send feature PRs to dev.');
}

export function checkCandidate(release, ref, expectedCommit) {
  assert.equal(release.isDraft, false);
  assert.equal(release.isPrerelease, true);
  assert.equal(ref.object.type, 'commit');
  assert.equal(ref.object.sha, expectedCommit, 'Tested candidate must match the exact dev head. Retest after changing dev.');
}

async function baseVersion() {
  const cargo = await readFile(resolve(root, 'Cargo.toml'), 'utf8');
  const base = cargo.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
  buildVersion(base, 'stable');
  return base;
}

async function prepare(channel) {
  const base = await baseVersion();
  const timestamp = new Date().toISOString().replace(/\D/g, '').slice(0, 14);
  const version = buildVersion(base, channel, process.env.GITHUB_RUN_NUMBER, process.env.GITHUB_RUN_ATTEMPT, timestamp);
  const sourceCommit = git('rev-parse', 'HEAD');
  const sourceDirty = git('status', '--porcelain').length > 0;
  if (channel !== 'local') {
    assert.equal(process.env.GITHUB_EVENT_NAME, 'push');
    assert.equal(process.env.GITHUB_REF, `refs/heads/${channel === 'dev' ? 'dev' : 'main'}`);
    assert.equal(sourceCommit, process.env.GITHUB_SHA);
    assert.equal(sourceDirty, false, 'Published packages require a clean source checkout.');
  }
  const metadata = {
    version, channel, sourceCommit, sourceDirty, target: 'x86_64-pc-windows-msvc',
    baseVersion: base, packageName: `goodtype-math-recognizer-${version}-windows-x64`, createdAt: new Date().toISOString(),
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

async function policy() {
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, 'utf8'));
  let pr = event.pull_request;
  if (!pr && process.env.GITHUB_REF === 'refs/heads/main' && process.env.GITHUB_EVENT_NAME === 'push') {
    const prs = JSON.parse(gh('api', `repos/${process.env.GITHUB_REPOSITORY}/commits/${process.env.GITHUB_SHA}/pulls`));
    pr = prs.find(p => p.merged_at && p.base.ref === 'main' && p.merge_commit_sha === process.env.GITHUB_SHA);
    assert.ok(pr, 'Main publication requires a merged dev-to-main PR.');
    const parents = git('show', '-s', '--format=%P', 'HEAD').split(' ');
    assert.equal(parents.length, 2, 'Promotion must use a merge commit.');
    assert.equal(parents[1], pr.head.sha);
    assert.equal(git('rev-parse', 'HEAD^{tree}'), git('rev-parse', `${pr.head.sha}^{tree}`), 'Main must rebuild the tested dev tree.');
  }
  if (!pr) return;
  assert.ok(['dev', 'main'].includes(pr.base.ref), 'Pull requests must target dev or main.');
  if (pr.base.ref !== 'main') return;
  checkPromotionBranch(pr, process.env.GITHUB_REPOSITORY);
  const tag = promotion(pr.body ?? '', await baseVersion());
  const repository = process.env.GITHUB_REPOSITORY;
  const release = JSON.parse(gh('release', 'view', tag, '--repo', repository, '--json', 'isDraft,isPrerelease,tagName'));
  const ref = JSON.parse(gh('api', `repos/${repository}/git/ref/tags/${tag}`));
  checkCandidate(release, ref, pr.head.sha);
  const runs = JSON.parse(gh('run', 'list', '--repo', repository, '--workflow', 'ci.yml', '--branch', 'dev',
    '--commit', pr.head.sha, '--event', 'push', '--status', 'success', '--json', 'databaseId'));
  assert.ok(runs.length > 0, 'Wait for the candidate build and all CI checks to pass.');
  // gh exits on auth/network errors; never interpret them as a missing stable version.
  const tags = JSON.parse(gh('api', '--paginate', '--slurp', `repos/${repository}/tags?per_page=100`)).flat();
  const stableTag = `v${await baseVersion()}`;
  assert.ok(!tags.some((item) => item.name === stableTag),
    'This stable version already exists. Bump Cargo.toml through dev.');
}

async function publish(directory) {
  const { readdir } = await import('node:fs/promises');
  const names = await readdir(directory);
  const manifests = names.filter((name) => /^goodtype-math-recognizer-.*-windows-x64\.json$/.test(name));
  assert.equal(manifests.length, 1);
  const metadata = JSON.parse(await readFile(resolve(directory, manifests[0]), 'utf8'));
  assert.equal(metadata.sourceCommit, process.env.GITHUB_SHA);
  assert.equal(metadata.sourceDirty, false);
  const channel = process.env.GITHUB_REF === 'refs/heads/dev' ? 'dev' : 'stable';
  assert.ok(['refs/heads/dev', 'refs/heads/main'].includes(process.env.GITHUB_REF));
  assert.equal(metadata.channel, channel);
  assert.equal(basename(metadata.archive.fileName), metadata.archive.fileName);
  assert.equal(await sha256(resolve(directory, metadata.archive.fileName)), metadata.archive.sha256);
  assert.equal(metadata.baseVersion, await baseVersion());
  assert.equal(metadata.version, buildVersion(metadata.baseVersion, channel, process.env.GITHUB_RUN_NUMBER, process.env.GITHUB_RUN_ATTEMPT));
  assert.deepEqual([...names].sort(), [metadata.archive.fileName, `${metadata.packageName}.json`, 'SHA256SUMS', 'recognizer-update.json', 'recognizer-update.json.sig', 'RELEASE_NOTES.md'].sort());
  const tag = `v${metadata.version}`;
  const repository = process.env.GITHUB_REPOSITORY;
  const tags = JSON.parse(gh('api', '--paginate', '--slurp', `repos/${repository}/tags?per_page=100`)).flat();
  assert.ok(!tags.some((item) => item.name === tag), `${tag} already exists. Published releases are never replaced.`);
  // Upload everything while the release is a draft; immutability locks assets on publication.
  const args = ['release', 'create', tag, '--draft', ...names.map((name) => resolve(directory, name)),
    '--repo', repository, '--target', metadata.sourceCommit, '--title', `Goodtype Ink Recognition ${tag}`,
    '--notes', `${channel === 'dev' ? 'Development candidate; test before promotion to main.' : 'Stable-channel rebuild of the tested dev source.'}\n\nSource: ${metadata.sourceCommit}\nBuild: ${metadata.buildUrl}\n\nWindows x64 portable ZIP. Extract the entire folder. See SHA256SUMS, the signed descriptor and included model provenance. Recognition quality and physical pen validation remain separate.`];
  args.push(...(channel === 'dev' ? ['--prerelease', '--latest=false'] : ['--latest']));
  gh(...args);
  gh('release', 'edit', tag, '--repo', repository, '--draft=false',
    channel === 'dev' ? '--latest=false' : '--latest');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, argument] = process.argv.slice(2);
  try {
    if (command === 'prepare') await prepare(argument);
    else if (command === 'policy') await policy();
    else if (command === 'publish') await publish(resolve(argument));
    else throw new Error('Usage: node scripts/release.mjs prepare local|dev|stable | policy | publish DIRECTORY');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
