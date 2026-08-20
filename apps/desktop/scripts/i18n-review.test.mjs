import assert from 'node:assert/strict';
import test from 'node:test';

import { parseReviewArguments } from './i18n-review/arguments.mjs';
import {
  createReviewReport,
  formatReviewReport,
  parseTranslationSource,
  REVIEW_LOCALES,
} from './i18n-review/report.mjs';

test('review report exposes all Luma and NVAPI messages for every translated locale', async () => {
  for (const locale of REVIEW_LOCALES) {
    const report = await createReviewReport(locale);
    assert.equal(report.messages.length, 274);
    assert.equal(new Set(report.messages.map(({ key }) => key)).size, 274);
    assert.ok(report.messages.every(({ source, translation }) => source && translation));
    assert.equal(report.editorialPolicy.nvidiaFamilyTerms !== undefined, true);
  }
});

test('review output is deterministic and includes policy metadata', async () => {
  const report = await createReviewReport('ru');
  assert.equal(formatReviewReport(report, 'json'), formatReviewReport(report, 'json'));
  const tsv = formatReviewReport(report, 'tsv');
  assert.match(tsv, /^key\tcontext\tsource\ttranslation\teditorial_policy\n/);
  assert.equal(tsv.trimEnd().split('\n').length, 275);
});

test('review CLI accepts only complete, unambiguous arguments', () => {
  assert.deepEqual(parseReviewArguments(['--locale', 'ru']), { locale: 'ru', format: 'tsv' });
  assert.deepEqual(parseReviewArguments(['--format', 'json', '--locale', 'de']), {
    locale: 'de',
    format: 'json',
  });

  for (const args of [
    [],
    ['--locale'],
    ['--locale', ''],
    ['--locale', 'ru', '--format', 'yaml'],
    ['--language', 'ru'],
    ['--locale', 'ru', '--locale', 'de'],
    ['--locale', 'ru', '--format', 'json', '--format', 'tsv'],
  ]) {
    assert.throws(() => parseReviewArguments(args), /Usage:/);
  }
});

test('review API rejects unsupported locales and output formats', async () => {
  await assert.rejects(() => createReviewReport('en'), /unsupported locale "en"/);
  const report = await createReviewReport('ru');
  assert.throws(() => formatReviewReport(report, 'yaml'), /unsupported format "yaml"/);
});

test('review parser rejects non-literal and duplicate translations', () => {
  assert.throws(
    () => parseTranslationSource('const translations = { key: makeValue() };'),
    /must be a string literal/,
  );
  assert.throws(
    () => parseTranslationSource("const translations = { key: 'one', key: 'two' };"),
    /duplicate translation/,
  );
});
