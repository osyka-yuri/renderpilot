import type { GameFileSafetyAssessment } from './types';
import { formatList } from '@shared/format';
import { getLocale, t, type MessageKeyForParams, type MessageKeyWithoutParams } from '@shared/i18n';

const ENGINE_LABELS: Record<string, string> = {
  EasyAntiCheat: 'Easy Anti-Cheat',
  easy_anti_cheat: 'Easy Anti-Cheat',
  'easyanti cheat': 'Easy Anti-Cheat',
  'Easy Anti-Cheat': 'Easy Anti-Cheat',
  BattlEye: 'BattlEye',
  battleye: 'BattlEye',
  battl_eye: 'BattlEye',
};

type FileSafetyPresentationInput = Pick<GameFileSafetyAssessment, 'detected_engines'> &
  Partial<Pick<GameFileSafetyAssessment, 'scan_completeness'>>;

export function presentDetectedEngine(engine: string): string {
  const trimmed = engine.trim();
  return ENGINE_LABELS[trimmed] ?? trimmed;
}

export function presentDetectedEngines(engines: readonly string[]): string[] {
  return [...new Set(engines.map(presentDetectedEngine).filter((engine) => engine.length > 0))];
}

export function fileSafetyMessageKey(
  assessment: FileSafetyPresentationInput | null | undefined,
):
  | MessageKeyWithoutParams
  | MessageKeyForParams<Readonly<{ engine: string | number }>>
  | MessageKeyForParams<Readonly<{ engines: string | number }>> {
  const engines = presentDetectedEngines(assessment?.detected_engines ?? []);
  if (engines.length === 1) {
    return 'gameDetails.fileSafety.detectedOne';
  }
  if (engines.length > 1) {
    return 'gameDetails.fileSafety.detectedMany';
  }
  return 'gameDetails.fileSafety.generic';
}

/** Formats detected engine names using the active locale's conjunction rules. */
export function formatDetectedEngines(engines: readonly string[], locale = getLocale()): string {
  return formatList(engines, locale);
}

export function presentFileSafetyMessage(
  assessment: FileSafetyPresentationInput | null | undefined,
): string {
  const engines = presentDetectedEngines(assessment?.detected_engines ?? []);
  const key = fileSafetyMessageKey(assessment);
  if (key === 'gameDetails.fileSafety.detectedOne') {
    return t(key, { engine: engines[0] ?? '' });
  }
  if (key === 'gameDetails.fileSafety.detectedMany') {
    return t(key, { engines: formatDetectedEngines(engines) });
  }
  return t(key);
}
