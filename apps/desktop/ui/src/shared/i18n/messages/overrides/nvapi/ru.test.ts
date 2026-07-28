import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

import { nvapiOverrides } from './ru';

type BackendSetting = {
  key: string;
  description?: string;
  values?: readonly { wire: string }[];
};

type BackendCatalog = {
  settings: readonly BackendSetting[];
};

const backendCatalog = JSON.parse(
  readFileSync(
    new URL(
      '../../../../../../../../../crates/renderpilot-orchestration/src/dlss/bundled/dlss_settings.json',
      import.meta.url,
    ),
    'utf8',
  ),
) as BackendCatalog;

function backendMessageKeys(): Set<string> {
  const keys = new Set<string>();

  for (const setting of backendCatalog.settings) {
    const prefix = `nvapi.${setting.key}`;
    keys.add(`${prefix}.label`);

    if (setting.description !== undefined) {
      keys.add(`${prefix}.description`);
    }

    for (const value of setting.values ?? []) {
      keys.add(`${prefix}.value.${value.wire}`);
    }
  }

  return keys;
}

describe('Russian NVAPI overrides', () => {
  it('contains only valid backend setting keys with non-empty string values', () => {
    const entries = Object.entries(nvapiOverrides);
    const validKeys = backendMessageKeys();

    for (const [key, value] of entries) {
      expect(validKeys.has(key)).toBe(true);
      expect(typeof value).toBe('string');
      expect(typeof value === 'string' ? value.trim().length : 0).toBeGreaterThan(0);
    }

    expect(new Set(entries.map(([key]) => key)).size).toBe(entries.length);
  });
});
