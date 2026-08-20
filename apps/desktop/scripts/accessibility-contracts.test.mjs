import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const startupHtmlUrl = new URL('../index.html', import.meta.url);

test('the pre-i18n startup indicator has a language-neutral accessible name', async () => {
  const html = await readFile(startupHtmlUrl, 'utf8');
  const startupIndicator = html.match(/<div[^>]+data-startup-skeleton[\s\S]*?<\/div>/)?.[0];

  assert.ok(startupIndicator, 'Expected the startup skeleton in index.html');
  assert.match(startupIndicator, /role="progressbar"/);
  assert.match(startupIndicator, /aria-label="RenderPilot"/);
  assert.doesNotMatch(startupIndicator, /aria-label="(?:Loading|Загрузка)\b/i);
});
