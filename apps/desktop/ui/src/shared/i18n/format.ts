import { createPluralRules } from '@shared/intl';

import type { Locale } from './locale';
import type { InterpolationParams, MessageValue, PluralForms } from './messages/types';

const cardinalPluralRules = createPluralRules({ type: 'cardinal' });

export function renderMessage(
  value: MessageValue,
  params: InterpolationParams | undefined,
  locale: Locale,
): string {
  if (typeof value === 'string') {
    return interpolateMessage(value, params);
  }

  const count = typeof params?.count === 'number' ? params.count : 0;
  const category = cardinalPluralRules(locale).select(count);
  const plural = value[category as keyof PluralForms] ?? value.other;

  return interpolateMessage(plural, params);
}

export function interpolateMessage(template: string, params?: InterpolationParams): string {
  if (!params) {
    return template;
  }

  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match,
  );
}
