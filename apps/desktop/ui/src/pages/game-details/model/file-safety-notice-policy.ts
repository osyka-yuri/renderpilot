import type { GameFileSafetyAssessment } from '@entities/game';

export type FileSafetyNoticeKind = 'general' | 'detected';

export type FileSafetyNotice = {
  kind: FileSafetyNoticeKind;
  assessment: GameFileSafetyAssessment;
};

/** An engine-specific warning takes precedence; otherwise show the general risk notice. */
export function fileSafetyNoticeKind(assessment: GameFileSafetyAssessment): FileSafetyNoticeKind {
  if (assessment.detected_engines.length > 0) {
    return 'detected';
  }
  return 'general';
}

export function createFileSafetyNotice(assessment: GameFileSafetyAssessment): FileSafetyNotice {
  return {
    kind: fileSafetyNoticeKind(assessment),
    assessment,
  };
}
