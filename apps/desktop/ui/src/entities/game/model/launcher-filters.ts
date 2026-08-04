import type { GameSummary } from './types';
import { normalizeUniqueTrimmedStrings } from '@shared/text';
import { hasPartialNormalizedSelection } from './selection-predicates';

export const ALL_KNOWN_LAUNCHERS = [
  'Steam',
  'Epic',
  'Gog',
  'Ubisoft',
  'Ea',
  'BattleNet',
  'Xbox',
  'Manual',
] as const;

export function normalizeLauncherValues(values: readonly string[]): string[] {
  return normalizeUniqueTrimmedStrings(values);
}

export function extractAvailableLaunchersFromCards(cards: readonly GameSummary[]): string[] {
  const launchers = new Set<string>();

  for (const card of cards) {
    const trimmed = card.launcher.trim();

    if (trimmed.length > 0) {
      launchers.add(trimmed);
    }
  }

  return Array.from(launchers).sort((left, right) => left.localeCompare(right));
}

export function hasPartialLauncherSelection(
  selectedLaunchers: readonly string[],
  availableLauncherValues: readonly string[],
): boolean {
  return hasPartialNormalizedSelection(
    normalizeLauncherValues(selectedLaunchers),
    normalizeLauncherValues(availableLauncherValues),
  );
}
