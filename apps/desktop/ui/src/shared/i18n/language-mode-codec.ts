import { DEFAULT_LANGUAGE_MODE, isLanguageMode, type LanguageMode } from './locale-model';

const STORAGE_VERSION = 2;
const LEGACY_LANGUAGE_MODES = ['system', 'en', 'ru', 'es', 'fr', 'de', 'ja', 'zh'] as const;

type LegacyLanguageMode = (typeof LEGACY_LANGUAGE_MODES)[number];

type StoredLanguageModeV2 = Readonly<{
  version: typeof STORAGE_VERSION;
  mode: LanguageMode;
}>;

export type DecodedStoredLanguageMode = Readonly<{
  mode: LanguageMode;
  migrate: boolean;
}>;

export function decodeStoredLanguageMode(value: string | null): DecodedStoredLanguageMode {
  if (value === null) {
    return { mode: DEFAULT_LANGUAGE_MODE, migrate: false };
  }

  if (isLegacyLanguageMode(value)) {
    return {
      mode: value === 'zh' ? 'zh-Hans' : value,
      migrate: true,
    };
  }

  try {
    const parsed: unknown = JSON.parse(value);
    if (isRecord(parsed) && parsed.version === STORAGE_VERSION && isLanguageMode(parsed.mode)) {
      return { mode: parsed.mode, migrate: false };
    }
  } catch {
    // Invalid JSON is handled by the safe default below.
  }

  return { mode: DEFAULT_LANGUAGE_MODE, migrate: false };
}

export function encodeStoredLanguageMode(mode: LanguageMode): string {
  const value: StoredLanguageModeV2 = { version: STORAGE_VERSION, mode };
  return JSON.stringify(value);
}

function isLegacyLanguageMode(value: string): value is LegacyLanguageMode {
  return LEGACY_LANGUAGE_MODES.includes(value as LegacyLanguageMode);
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
