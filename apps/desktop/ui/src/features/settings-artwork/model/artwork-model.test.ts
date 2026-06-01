import { describe, expect, it } from 'vitest';
import {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
} from '@entities/settings';
import { coverSourceToggleRows, formatBooleanSetting } from './artwork-model';

import { createInitialSettingsArtworkState } from './artwork-state';

const expectedCoverPolicyBySetting = {
  [COVERS_GOG_CDN_SETTING_KEY]: 'gogCdn',
  [COVERS_STEAM_CDN_SETTING_KEY]: 'steamCdn',
  [COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY]: 'steamgriddb',
} as const satisfies Record<string, keyof CoverRemotePolicy>;

type CoverRemotePolicy = ReturnType<typeof createInitialSettingsArtworkState>['coverSourcesState'];

function expectUniqueValues(values: readonly unknown[]): void {
  expect(new Set(values).size).toBe(values.length);
}

function expectNonEmptyString(value: string): void {
  expect(value.trim()).toBe(value);
  expect(value).not.toHaveLength(0);
}

describe('artwork-model', () => {
  describe('formatBooleanSetting', () => {
    it.each([
      [true, 'true'],
      [false, 'false'],
    ] as const)('formats %s for catalog persistence', (value, expected) => {
      expect(formatBooleanSetting(value)).toBe(expected);
    });
  });

  describe('coverSourceToggleRows', () => {
    it('declares one row for every supported cover source setting', () => {
      expect(coverSourceToggleRows).toHaveLength(Object.keys(expectedCoverPolicyBySetting).length);

      expectUniqueValues(coverSourceToggleRows.map((row) => row.settingKey));
    });

    it('binds each catalog setting key to the expected policy key', () => {
      const actualPolicyBySetting = Object.fromEntries(
        coverSourceToggleRows.map((row) => [row.settingKey, row.policyKey]),
      );

      expect(actualPolicyBySetting).toEqual(expectedCoverPolicyBySetting);
    });

    it('points every row to an existing default cover source policy', () => {
      const defaultState = createInitialSettingsArtworkState().coverSourcesState;

      for (const row of coverSourceToggleRows) {
        expect(defaultState).toHaveProperty(row.policyKey);
        expect(typeof defaultState[row.policyKey]).toBe('boolean');
      }
    });

    it('keeps cover source policy keys unique', () => {
      expectUniqueValues(coverSourceToggleRows.map((row) => row.policyKey));
    });

    it('provides complete non-empty UI metadata for every cover source row', () => {
      for (const row of coverSourceToggleRows) {
        expectNonEmptyString(row.ariaLabelKey);
        expectNonEmptyString(row.eyebrow);
        expectNonEmptyString(row.titleKey);
        expectNonEmptyString(row.descriptionKey);
      }
    });
  });
});
