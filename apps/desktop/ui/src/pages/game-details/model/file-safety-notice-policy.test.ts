import { describe, expect, it } from 'vitest';

import { createFileSafetyNotice, fileSafetyNoticeKind } from './file-safety-notice-policy';

const assessment = {
  game_id: 'steam:123',
  context_token: 'context-token',
  detected_engines: [] as string[],
  scan_completeness: 'complete' as const,
};

describe('file safety notice policy', () => {
  it('uses the general notice whenever there is no detected engine', () => {
    expect(fileSafetyNoticeKind(assessment)).toBe('general');
    expect(fileSafetyNoticeKind({ ...assessment, scan_completeness: 'limited' })).toBe('general');
    expect(
      fileSafetyNoticeKind({
        ...assessment,
        scan_completeness: 'limited',
        detected_engines: ['BattlEye'],
      }),
    ).toBe('detected');
    expect(fileSafetyNoticeKind({ ...assessment, detected_engines: ['BattlEye'] })).toBe(
      'detected',
    );
  });

  it('creates a notice with the current kind and assessment', () => {
    const currentAssessment = { ...assessment, scan_completeness: 'limited' as const };
    const notice = createFileSafetyNotice(currentAssessment);

    expect(notice).toEqual({ kind: 'general', assessment: currentAssessment });
  });
});
