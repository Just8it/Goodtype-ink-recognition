import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildVersion, checkPromotionBranch, checkCandidate, promotion, sha256 } from './release.mjs';

test('each dev run and retry has a distinct version while stable preserves the base', () => {
  assert.equal(buildVersion('0.1.0', 'dev', '12', '1'), '0.1.0-dev.12.1');
  assert.notEqual(buildVersion('0.1.0', 'dev', '12', '2'), buildVersion('0.1.0', 'dev', '12', '1'));
  assert.notEqual(buildVersion('0.1.0', 'dev', '13', '1'), buildVersion('0.1.0', 'dev', '12', '1'));
  assert.equal(buildVersion('0.1.0', 'stable'), '0.1.0');
  assert.equal(buildVersion('0.1.0', 'local', null, null, '20261027143000'), '0.1.0-local.20261027143000');
});

test('versions cannot introduce paths, invalid SemVer or missing CI identity', () => {
  for (const base of ['../0.1.0', '01.1.0', '0.1', '0.1.0-beta', '0.1.0\n']) {
    assert.throws(() => buildVersion(base, 'stable'));
  }
  for (const run of [undefined, '0', '../1', '01', '1\n']) {
    assert.throws(() => buildVersion('0.1.0', 'dev', run, '1'));
  }
  assert.throws(() => buildVersion('0.1.0', 'other'));
});

test('promotion requires the correct versioned candidate and recorded test evidence', () => {
  const body = 'Tested dev build: v0.1.0-dev.12.1\nTest evidence: ZIP hashes and relocated decoder passed; Windows walkthrough recorded.';
  assert.equal(promotion(body, '0.1.0'), 'v0.1.0-dev.12.1');
  for (const invalid of ['', body.replace('0.1.0', '0.2.0'), body.split('\n')[0], body.replace('v0.1.0-dev.12.1', 'v0.1.0')]) {
    assert.throws(() => promotion(invalid, '0.1.0'));
  }
  assert.throws(() => promotion(`<!-- ${body} -->`, '0.1.0'));
});

test('checksums identify content changes in release assets', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'goodtype-release-test-'));
  try {
    const path = join(directory, 'asset');
    await writeFile(path, 'abc');
    assert.equal(await sha256(path), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
    await writeFile(path, 'abd');
    assert.notEqual(await sha256(path), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  } finally {
    await rm(directory, { recursive: true });
  }
});

test('changing dev after testing invalidates promotion; drafts and stable tags are not candidates', () => {
  const release = { isDraft: false, isPrerelease: true };
  const ref = { object: { type: 'commit', sha: 'a'.repeat(40) } };
  checkCandidate(release, ref, 'a'.repeat(40));
  assert.throws(() => checkCandidate(release, ref, 'b'.repeat(40)), /exact dev head/);
  assert.throws(() => checkCandidate({ ...release, isDraft: true }, ref, ref.object.sha));
  assert.throws(() => checkCandidate({ ...release, isPrerelease: false }, ref, ref.object.sha));
  assert.throws(() => checkCandidate(release, { object: { ...ref.object, type: 'tag' } }, ref.object.sha));
});

test('main accepts only the same repository dev branch', () => {
  const pr = { head: { repo: { full_name: 'Just8it/Goodtype-ink-recognition' }, ref: 'dev' } };
  checkPromotionBranch(pr, pr.head.repo.full_name);
  assert.throws(() => checkPromotionBranch({head: {...pr.head, ref: 'feature/test'}}, pr.head.repo.full_name));
  assert.throws(() => checkPromotionBranch(pr, 'another/repository'));
});
