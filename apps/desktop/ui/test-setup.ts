import { setLanguageMode } from '@shared/i18n';

// Unit tests assert against the English catalog. Pin the locale to 'en' so
// results are deterministic regardless of the host machine's system language
// (otherwise `resolveLocale('system')` follows navigator.language).
setLanguageMode('en');
