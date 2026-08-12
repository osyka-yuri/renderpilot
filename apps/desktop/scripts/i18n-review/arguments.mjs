import { REVIEW_FORMATS } from './report.mjs';

const USAGE = 'Usage: pnpm i18n:review --locale <locale> [--format tsv|json]';

function fail() {
  throw new Error(USAGE);
}

export function parseReviewArguments(args) {
  let locale;
  let format = 'tsv';
  const seenOptions = new Set();

  for (let index = 0; index < args.length; index += 2) {
    const option = args[index];
    const value = args[index + 1];
    if (
      value === undefined ||
      typeof value !== 'string' ||
      value.trim() === '' ||
      (option !== '--locale' && option !== '--format') ||
      seenOptions.has(option)
    ) {
      fail();
    }
    seenOptions.add(option);
    if (option === '--locale') {
      locale = value;
    }
    if (option === '--format') {
      format = value;
    }
  }

  if (locale === undefined || !REVIEW_FORMATS.includes(format)) {
    fail();
  }
  return { locale, format };
}
