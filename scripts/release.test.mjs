import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { requireReleaseTag, sha256 } from './release.mjs';

test('only a push of the matching version tag may publish', () => {
  requireReleaseTag('0.1.1', 'push', 'refs/tags/v0.1.1');
  assert.throws(() => requireReleaseTag('0.1.1', 'push', 'refs/heads/main'));
  assert.throws(() => requireReleaseTag('0.1.1', 'push', 'refs/tags/v0.1.0'));
  assert.throws(() => requireReleaseTag('0.1.1', 'workflow_dispatch', 'refs/tags/v0.1.1'));
});

test('hashes asset bytes', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'goodtype-release-'));
  try {
    const asset = join(directory, 'asset.zip');
    await writeFile(asset, 'abc');
    assert.equal(await sha256(asset), 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
