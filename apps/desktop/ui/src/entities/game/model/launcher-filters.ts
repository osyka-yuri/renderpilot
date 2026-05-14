import type { GameSummary } from './types';
import { normalizeUniqueTrimmedStrings } from '@shared/text';

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

  return [...launchers].sort((left, right) => left.localeCompare(right));
}

export function hasPartialLauncherSelection(
  selectedLaunchers: readonly string[],
  availableLauncherValues: readonly string[],
): boolean {
  const availableLaunchers = normalizeLauncherValues(availableLauncherValues);

  if (availableLaunchers.length === 0) {
    return false;
  }

  const selectedAvailableLaunchers = intersectNormalizedLaunchers(
    normalizeLauncherValues(selectedLaunchers),
    availableLaunchers,
  );

  return selectedAvailableLaunchers.length < availableLaunchers.length;
}

function intersectNormalizedLaunchers(
  selection: readonly string[],
  available: readonly string[],
): string[] {
  if (available.length === 0) {
    return [];
  }

  const allowedLaunchers = new Set(available);

  return selection.filter((launcher) => allowedLaunchers.has(launcher));
}
