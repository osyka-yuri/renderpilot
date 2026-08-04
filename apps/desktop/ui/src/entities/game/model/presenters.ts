import { humanizeToken, takeGraphemePrefix } from '@shared/text';
import type { Locale } from '@shared/i18n';

const LAUNCHER_LABELS: Record<string, string> = {
  NativeWindows: 'Native Windows',
  NativeLinux: 'Native Linux',
  BattleNet: 'Battle.net',
  MacOs: 'macOS',
  gog: 'GOG',
};

export function formatLauncherLabel(value?: string | null): string {
  if (!value) {
    return 'Unknown';
  }

  return LAUNCHER_LABELS[value] ?? humanizeToken(value);
}

export function formatLauncher(value?: string | null): string {
  return formatLauncherLabel(value);
}

const DEFAULT_TITLE_MONOGRAM = 'RP';
const MONOGRAM_SINGLE_WORD_LENGTH = 2;
const MONOGRAM_WORDS_LIMIT = 2;

export function titleMonogram(title: string, locale: Locale): string {
  const words = getTitleWords(title);

  if (words.length === 0) {
    return DEFAULT_TITLE_MONOGRAM;
  }

  if (words.length === 1) {
    return takeGraphemePrefix(words[0], MONOGRAM_SINGLE_WORD_LENGTH, locale).toUpperCase();
  }

  return words
    .slice(0, MONOGRAM_WORDS_LIMIT)
    .map((word) => takeGraphemePrefix(word, 1, locale))
    .join('')
    .toUpperCase();
}

function getTitleWords(title: string): string[] {
  return title.trim().split(/\s+/).filter(Boolean);
}
