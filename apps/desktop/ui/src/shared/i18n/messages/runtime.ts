import { createPluralRules } from '@shared/intl';

import type { Locale } from '../locale';
import type { InterpolationParams, MessageValue } from './model';
import { analyzeMessageTemplate, type MessageTemplateAnalysis } from './template';

export { analyzeMessageTemplate };
export type RuntimeTemplateAnalysis = MessageTemplateAnalysis;

const cardinalPluralRules = createPluralRules({ type: 'cardinal' });

export function interpolateMessage(template: string, params?: InterpolationParams): string {
  if (!params || (!template.includes('{') && !template.includes('}'))) {
    return template;
  }

  const analysis = analyzeMessageTemplate(template);
  if (!analysis.valid) {
    return template;
  }

  return analysis.tokens
    .map((token) => {
      if (token.kind === 'text') {
        return token.value;
      }
      return Object.prototype.hasOwnProperty.call(params, token.name)
        ? String(params[token.name])
        : `{${token.name}}`;
    })
    .join('');
}

export function renderMessage(
  value: MessageValue,
  params: InterpolationParams | undefined,
  locale: Locale,
): string {
  if (typeof value === 'string') {
    return interpolateMessage(value, params);
  }

  if (value.kind === 'plural') {
    const argument = params?.[value.argument];
    const count = typeof argument === 'number' && Number.isFinite(argument) ? argument : 0;
    const category = cardinalPluralRules(locale).select(count);
    const template = value.forms[category] ?? value.forms.other;

    return interpolateMessage(template, params);
  }

  const argument = params?.[value.argument];
  const template = typeof argument === 'string' ? value.cases[argument] : undefined;

  return interpolateMessage(template ?? value.cases.other, params);
}
