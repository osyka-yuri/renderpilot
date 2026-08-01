import { describe, expect, it } from 'vitest';

import { plural, select } from './model';
import { PLACEHOLDER_CONTRACT_CASES } from './placeholder-contract-cases';
import { analyzeMessageTemplate, interpolateMessage, renderMessage } from './runtime';

describe('message runtime', () => {
  it('matches the shared placeholder contract table', () => {
    for (const fixture of PLACEHOLDER_CONTRACT_CASES) {
      const analysis = analyzeMessageTemplate(fixture.template);
      expect(analysis.valid, fixture.template).toBe(fixture.valid);
      expect(analysis.placeholders, fixture.template).toEqual(fixture.placeholders);
    }
  });

  it('interpolates valid placeholders and preserves missing values', () => {
    expect(interpolateMessage('Hello, {name}. Code: {code_2}.', { name: 'Ada', code_2: 42 })).toBe(
      'Hello, Ada. Code: 42.',
    );
    expect(interpolateMessage('{known} {missing}', { known: 'yes' })).toBe('yes {missing}');
    expect(interpolateMessage('{name} / {name}', { name: 'Ada' })).toBe('Ada / Ada');
  });

  it('reads only own parameter properties', () => {
    const inherited = Object.create({ name: 'prototype value' }) as Readonly<
      Record<string, string | number>
    >;

    expect(interpolateMessage('Hello, {name}.', inherited)).toBe('Hello, {name}.');
  });

  it('fails closed without partially interpolating an invalid external template', () => {
    expect(interpolateMessage('{valid} then {bad-name}', { valid: 'replaced' })).toBe(
      '{valid} then {bad-name}',
    );
    expect(interpolateMessage('{{name}}', { name: 'Ada' })).toBe('{{name}}');
  });

  it('uses the plural message argument instead of a hard-coded count key', () => {
    const value = plural('items', {
      one: '{items} item',
      other: '{items} items',
    });

    expect(renderMessage(value, { items: 1 }, 'en')).toBe('1 item');
    expect(renderMessage(value, { items: 3 }, 'en')).toBe('3 items');
  });

  it('selects locale-specific plural categories and falls back to other', () => {
    const value = plural('amount', {
      one: '{amount} файл',
      few: '{amount} файла',
      many: '{amount} файлов',
      other: '{amount} файла',
    });

    expect(renderMessage(value, { amount: 1 }, 'ru')).toBe('1 файл');
    expect(renderMessage(value, { amount: 2 }, 'ru')).toBe('2 файла');
    expect(renderMessage(value, { amount: 5 }, 'ru')).toBe('5 файлов');
    expect(renderMessage(value, { amount: 1.5 }, 'ru')).toBe('1.5 файла');
  });

  it('uses a safe zero for an absent or non-finite plural argument', () => {
    const value = plural('amount', {
      one: 'one',
      other: 'other',
    });

    expect(renderMessage(value, undefined, 'en')).toBe('other');
    expect(renderMessage(value, { amount: Number.NaN }, 'en')).toBe('other');
    expect(renderMessage(value, { amount: Number.POSITIVE_INFINITY }, 'en')).toBe('other');
  });

  it('renders known select cases and uses other for unknown or absent values', () => {
    const value = select('tone', {
      formal: 'Welcome, {name}',
      casual: 'Hi, {name}',
      other: 'Hello, {name}',
    });

    expect(renderMessage(value, { tone: 'formal', name: 'Ada' }, 'en')).toBe('Welcome, Ada');
    expect(renderMessage(value, { tone: 'unknown', name: 'Ada' }, 'en')).toBe('Hello, Ada');
    expect(renderMessage(value, { name: 'Ada' }, 'en')).toBe('Hello, Ada');
  });
});
