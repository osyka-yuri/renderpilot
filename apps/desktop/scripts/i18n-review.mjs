import { parseReviewArguments } from './i18n-review/arguments.mjs';
import { createReviewReport, formatReviewReport } from './i18n-review/report.mjs';

const { locale, format } = parseReviewArguments(process.argv.slice(2));
process.stdout.write(formatReviewReport(await createReviewReport(locale), format));
