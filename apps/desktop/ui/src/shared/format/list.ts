import type { Locale } from '@shared/i18n';
import { createListFormatter } from '@shared/intl';

const conjunctionListFormatter = createListFormatter({
  type: 'conjunction',
  style: 'long',
});

export function formatList(values: readonly string[], locale: Locale): string {
  return conjunctionListFormatter(locale).format(values);
}
