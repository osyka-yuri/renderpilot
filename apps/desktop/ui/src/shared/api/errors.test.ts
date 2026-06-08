import { describe, expect, it } from 'vitest';

import {
  DesktopCommandError,
  describeCommandError,
  describeCommandErrorBrief,
  describeCommandErrorTechnical,
  normalizeCommandError,
} from './errors';
import type { CommandErrorDto } from './types';

// An unknown message key forces `translateKey` to fall back to `details`, so the
// assertions stay independent of the i18n catalog contents.
function dto(overrides: Partial<CommandErrorDto> = {}): CommandErrorDto {
  return {
    code: 'storage_failed',
    severity: 'error',
    messageKey: 'test.unknown.message',
    details: 'Disk is full',
    suggestedActions: [],
    ...overrides,
  };
}

describe('normalizeCommandError', () => {
  it('returns the same instance when given a DesktopCommandError', () => {
    const original = normalizeCommandError(dto());
    expect(normalizeCommandError(original)).toBe(original);
  });

  it('preserves a well-formed backend DTO', () => {
    const err = normalizeCommandError(dto({ code: 'provider_failed', details: 'No source' }));

    expect(err).toBeInstanceOf(DesktopCommandError);
    expect(err.dto.code).toBe('provider_failed');
    expect(err.dto.severity).toBe('error');
    expect(err.dto.details).toBe('No source');
  });

  it('wraps a malformed DTO (invalid severity) as a generic fallback', () => {
    const err = normalizeCommandError({ ...dto(), severity: 'fatal' });

    expect(err.dto.code).toBe('command_failed');
    expect(err.dto.severity).toBe('error');
  });

  it('wraps a thrown Error using its message as the technical detail', () => {
    const err = normalizeCommandError(new Error('boom'));

    expect(err.dto.code).toBe('command_failed');
    expect(describeCommandErrorTechnical(err)).toBe('boom');
  });

  it('wraps and trims a string error', () => {
    expect(describeCommandErrorTechnical(normalizeCommandError('  oops  '))).toBe('oops');
  });

  it('stringifies non-error, non-string values', () => {
    expect(describeCommandErrorTechnical(normalizeCommandError(null))).toBe('null');
    expect(describeCommandErrorTechnical(normalizeCommandError(undefined))).toBe('undefined');
    expect(describeCommandErrorTechnical(normalizeCommandError(42))).toBe('42');
  });
});

describe('describeCommandError variants', () => {
  it('brief returns the details for an unknown message key', () => {
    expect(describeCommandErrorBrief(dto({ details: 'Disk is full' }))).toBe('Disk is full');
  });

  it('full message appends suggested actions; brief omits them', () => {
    const withActions = dto({
      details: 'Disk is full',
      suggestedActions: [{ key: 'test.unknown.action', text: 'Free up space' }],
    });

    expect(describeCommandError(withActions)).toBe('Disk is full Free up space');
    expect(describeCommandErrorBrief(withActions)).toBe('Disk is full');
  });

  it('technical prefers debugDetails, falling back to details when blank', () => {
    expect(
      describeCommandErrorTechnical(dto({ details: 'Generic', debugDetails: 'exact cause' })),
    ).toBe('exact cause');
    expect(describeCommandErrorTechnical(dto({ details: 'Generic', debugDetails: '   ' }))).toBe(
      'Generic',
    );
  });
});

describe('DesktopCommandError', () => {
  it('normalizes blank fields to safe fallbacks', () => {
    const err = new DesktopCommandError({
      code: '',
      severity: 'warning',
      messageKey: '',
      details: '',
      suggestedActions: [],
    });

    expect(err.dto.code).toBe('command_failed');
    expect(err.dto.messageKey).toBe('errors.command_failed');
    expect(err.dto.severity).toBe('warning');
    expect(err.dto.details).toBe('Unknown command error');
  });

  it('drops suggested actions whose key and text are both blank', () => {
    const err = new DesktopCommandError({
      code: 'storage_failed',
      severity: 'error',
      messageKey: 'test.unknown.message',
      details: 'Disk is full',
      suggestedActions: [{ key: '  ', text: '  ' }],
    });

    expect(err.dto.suggestedActions).toEqual([]);
  });
});
