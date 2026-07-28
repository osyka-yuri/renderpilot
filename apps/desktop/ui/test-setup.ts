import { beforeEach } from 'vitest';

// Replace Node/jsdom variants consistently: some Node builds expose an
// experimental localStorage whose getter emits warnings before tests even run.
const values = new Map<string, string>();
Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    },
    clear: () => {
      values.clear();
    },
    key: (index: number) => Array.from(values.keys())[index] ?? null,
    get length() {
      return values.size;
    },
  },
});

const { setLanguageMode } = await import('@shared/i18n');

// Unit tests assert against the English catalog. Pin the locale to 'en' so
// results are deterministic regardless of the host machine's system language
// (otherwise `resolveLocale('system')` follows navigator.language).
await setLanguageMode('en');

beforeEach(async () => {
  values.clear();
  await setLanguageMode('en');
});
