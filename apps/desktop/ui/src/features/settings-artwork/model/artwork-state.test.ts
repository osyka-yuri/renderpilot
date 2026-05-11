import { describe, expect, it } from 'vitest';
import { coverSourceToggleRows } from './artwork-model';
import type { CoverRemotePolicy } from '@entities/settings';
import type { CoverSourcePolicyKey, CoverSourceToggleRow } from './artwork-model';
import {
  beginCoverSourceMutation,
  createInitialSettingsArtworkState,
  isCoverSourceDisabled,
  isCoverSourceSaving,
  isCurrentCoverSourceMutation,
  nextCoverSourceMutationVersion,
  withCoverSourcesBusy,
  withCoverSourcesLoaded,
  withCoverSourcesPolicy,
  withCoverSourceSaving,
  withCoverSourceValue,
} from './artwork-state';

function getCoverSourceRow(policyKey: CoverSourcePolicyKey): CoverSourceToggleRow {
  const row = coverSourceToggleRows.find((item) => item.policyKey === policyKey);

  if (!row) {
    throw new Error(`Missing cover source toggle row for policy key: ${policyKey}`);
  }

  return row;
}

function createLoadedArtworkState() {
  return withCoverSourcesLoaded(createInitialSettingsArtworkState(), true);
}

function invertCoverSourcesPolicy(policy: CoverRemotePolicy): CoverRemotePolicy {
  return {
    steamCdn: !policy.steamCdn,
    gogCdn: !policy.gogCdn,
    steamgriddb: !policy.steamgriddb,
  };
}

describe('settings-page-state', () => {
  describe('createInitialSettingsArtworkState', () => {
    it('creates independent initial state instances', () => {
      const first = createInitialSettingsArtworkState();
      const second = createInitialSettingsArtworkState();

      expect(first).not.toBe(second);
      expect(first.coverSourcesState).not.toBe(second.coverSourcesState);
      expect(first.savingCoverSourceKeys).not.toBe(second.savingCoverSourceKeys);
      expect(first.coverSourceMutationVersion).not.toBe(second.coverSourceMutationVersion);
    });

    it('starts as not loaded, not busy, and without saving keys', () => {
      const state = createInitialSettingsArtworkState();

      expect(state.coverSourcesLoaded).toBe(false);
      expect(state.coverSourcesBusy).toBe(false);
      expect(state.savingCoverSourceKeys.size).toBe(0);
      expect(state.coverSourceMutationVersion).toEqual({});
    });
  });

  describe('cover source value', () => {
    it('updates cover source value immutably', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const nextEnabled = !initial.coverSourcesState[row.policyKey];

      const updated = withCoverSourceValue(initial, row.policyKey, nextEnabled);

      expect(updated).not.toBe(initial);
      expect(updated.coverSourcesState).not.toBe(initial.coverSourcesState);
      expect(updated.coverSourcesState[row.policyKey]).toBe(nextEnabled);
      expect(initial.coverSourcesState[row.policyKey]).toBe(!nextEnabled);
    });

    it('keeps unrelated state references when only cover source value changes', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const nextEnabled = !initial.coverSourcesState[row.policyKey];

      const updated = withCoverSourceValue(initial, row.policyKey, nextEnabled);

      expect(updated.savingCoverSourceKeys).toBe(initial.savingCoverSourceKeys);
      expect(updated.coverSourceMutationVersion).toBe(initial.coverSourceMutationVersion);
    });

    it('returns the same state when cover source value is unchanged', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();

      const updated = withCoverSourceValue(
        initial,
        row.policyKey,
        initial.coverSourcesState[row.policyKey],
      );

      expect(updated).toBe(initial);
    });
  });

  describe('cover source loaded and busy flags', () => {
    it('updates loaded flag only when value changes', () => {
      const initial = createInitialSettingsArtworkState();

      const loaded = withCoverSourcesLoaded(initial, true);
      const sameLoaded = withCoverSourcesLoaded(loaded, true);

      expect(loaded).not.toBe(initial);
      expect(loaded.coverSourcesLoaded).toBe(true);
      expect(sameLoaded).toBe(loaded);
    });

    it('updates busy flag only when value changes', () => {
      const initial = createInitialSettingsArtworkState();

      const busy = withCoverSourcesBusy(initial, true);
      const sameBusy = withCoverSourcesBusy(busy, true);

      expect(busy).not.toBe(initial);
      expect(busy.coverSourcesBusy).toBe(true);
      expect(sameBusy).toBe(busy);
    });
  });

  describe('cover source policy', () => {
    it('replaces policy immutably', () => {
      const initial = createInitialSettingsArtworkState();
      const policy = invertCoverSourcesPolicy(initial.coverSourcesState);

      const updated = withCoverSourcesPolicy(initial, policy);

      expect(updated).not.toBe(initial);
      expect(updated.coverSourcesState).not.toBe(initial.coverSourcesState);
      expect(updated.coverSourcesState).toEqual(policy);
      expect(initial.coverSourcesState).not.toEqual(policy);
    });

    it('returns the same state when policy is unchanged', () => {
      const initial = createInitialSettingsArtworkState();

      const updated = withCoverSourcesPolicy(initial, initial.coverSourcesState);

      expect(updated).toBe(initial);
    });

    it('does not keep a mutable reference to incoming policy object', () => {
      const initial = createInitialSettingsArtworkState();
      const policy = invertCoverSourcesPolicy(initial.coverSourcesState);
      const expectedPolicy = { ...policy };

      const updated = withCoverSourcesPolicy(initial, policy);

      policy.steamCdn = !policy.steamCdn;
      policy.gogCdn = !policy.gogCdn;
      policy.steamgriddb = !policy.steamgriddb;

      expect(updated.coverSourcesState).toEqual(expectedPolicy);
    });
  });

  describe('saving state', () => {
    it('tracks per-setting saving state', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();

      const saving = withCoverSourceSaving(initial, row.settingKey, true);
      const done = withCoverSourceSaving(saving, row.settingKey, false);

      expect(isCoverSourceSaving(saving, row.settingKey)).toBe(true);
      expect(isCoverSourceSaving(done, row.settingKey)).toBe(false);
    });

    it('does not mutate previous saving key set instance', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const initialSavingKeys = initial.savingCoverSourceKeys;

      const saving = withCoverSourceSaving(initial, row.settingKey, true);

      expect(initialSavingKeys.has(row.settingKey)).toBe(false);
      expect(saving.savingCoverSourceKeys.has(row.settingKey)).toBe(true);
      expect(saving.savingCoverSourceKeys).not.toBe(initialSavingKeys);
    });

    it('returns the same state when saving flag is unchanged', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();

      const notSaving = withCoverSourceSaving(initial, row.settingKey, false);
      const saving = withCoverSourceSaving(initial, row.settingKey, true);
      const sameSaving = withCoverSourceSaving(saving, row.settingKey, true);

      expect(notSaving).toBe(initial);
      expect(sameSaving).toBe(saving);
    });

    it('clears only the requested saving key', () => {
      const steamRow = getCoverSourceRow('steamCdn');
      const gogRow = getCoverSourceRow('gogCdn');

      const initial = createInitialSettingsArtworkState();
      const savingSteam = withCoverSourceSaving(initial, steamRow.settingKey, true);
      const savingBoth = withCoverSourceSaving(savingSteam, gogRow.settingKey, true);

      const onlyGogSaving = withCoverSourceSaving(savingBoth, steamRow.settingKey, false);

      expect(isCoverSourceSaving(onlyGogSaving, steamRow.settingKey)).toBe(false);
      expect(isCoverSourceSaving(onlyGogSaving, gogRow.settingKey)).toBe(true);
    });
  });

  describe('disabled state', () => {
    it('disables toggles while policy is not loaded', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();

      expect(isCoverSourceDisabled(initial, row)).toBe(true);
    });

    it('enables toggles after policy is loaded', () => {
      const row = getCoverSourceRow('steamCdn');
      const loaded = createLoadedArtworkState();

      expect(isCoverSourceDisabled(loaded, row)).toBe(false);
    });

    it('disables toggles while policy is busy', () => {
      const row = getCoverSourceRow('steamCdn');
      const loaded = createLoadedArtworkState();

      const busy = withCoverSourcesBusy(loaded, true);

      expect(isCoverSourceDisabled(busy, row)).toBe(true);
    });

    it('disables only toggle with matching saving key', () => {
      const steamRow = getCoverSourceRow('steamCdn');
      const gogRow = getCoverSourceRow('gogCdn');
      const loaded = createLoadedArtworkState();

      const savingSteam = withCoverSourceSaving(loaded, steamRow.settingKey, true);

      expect(isCoverSourceDisabled(savingSteam, steamRow)).toBe(true);
      expect(isCoverSourceDisabled(savingSteam, gogRow)).toBe(false);
    });
  });

  describe('mutation version', () => {
    it('increments mutation version per setting key independently', () => {
      const steamRow = getCoverSourceRow('steamCdn');
      const gogRow = getCoverSourceRow('gogCdn');
      const initial = createInitialSettingsArtworkState();

      const firstSteam = nextCoverSourceMutationVersion(initial, steamRow.settingKey);
      const firstGog = nextCoverSourceMutationVersion(firstSteam.state, gogRow.settingKey);
      const secondSteam = nextCoverSourceMutationVersion(firstGog.state, steamRow.settingKey);

      expect(firstSteam.version).toBe(1);
      expect(firstGog.version).toBe(1);
      expect(secondSteam.version).toBe(2);

      expect(
        isCurrentCoverSourceMutation(secondSteam.state, steamRow.settingKey, secondSteam.version),
      ).toBe(true);

      expect(
        isCurrentCoverSourceMutation(secondSteam.state, steamRow.settingKey, firstSteam.version),
      ).toBe(false);
    });

    it('does not mutate previous mutation version object', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const initialVersions = initial.coverSourceMutationVersion;

      const next = nextCoverSourceMutationVersion(initial, row.settingKey);

      expect(initialVersions[row.settingKey]).toBeUndefined();
      expect(next.state.coverSourceMutationVersion[row.settingKey]).toBe(1);
      expect(next.state.coverSourceMutationVersion).not.toBe(initialVersions);
    });
  });

  describe('beginCoverSourceMutation', () => {
    it('begins mutation atomically with version, saving state, and optimistic value', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const nextEnabled = !initial.coverSourcesState[row.policyKey];

      const result = beginCoverSourceMutation({
        state: initial,
        row,
        nextEnabled,
      });

      expect(result.version).toBe(1);
      expect(result.state.coverSourcesState[row.policyKey]).toBe(nextEnabled);
      expect(isCoverSourceSaving(result.state, row.settingKey)).toBe(true);
      expect(isCurrentCoverSourceMutation(result.state, row.settingKey, 1)).toBe(true);
    });

    it('does not mutate previous state when mutation begins', () => {
      const row = getCoverSourceRow('steamCdn');
      const initial = createInitialSettingsArtworkState();
      const initialValue = initial.coverSourcesState[row.policyKey];

      const result = beginCoverSourceMutation({
        state: initial,
        row,
        nextEnabled: !initialValue,
      });

      expect(result.state).not.toBe(initial);
      expect(result.state.coverSourcesState).not.toBe(initial.coverSourcesState);
      expect(result.state.savingCoverSourceKeys).not.toBe(initial.savingCoverSourceKeys);
      expect(result.state.coverSourceMutationVersion).not.toBe(initial.coverSourceMutationVersion);

      expect(initial.coverSourcesState[row.policyKey]).toBe(initialValue);
      expect(initial.savingCoverSourceKeys.has(row.settingKey)).toBe(false);
      expect(initial.coverSourceMutationVersion[row.settingKey]).toBeUndefined();
    });

    it('preserves existing saving keys when another mutation begins', () => {
      const steamRow = getCoverSourceRow('steamCdn');
      const gogRow = getCoverSourceRow('gogCdn');

      const initial = withCoverSourceSaving(
        createInitialSettingsArtworkState(),
        gogRow.settingKey,
        true,
      );

      const result = beginCoverSourceMutation({
        state: initial,
        row: steamRow,
        nextEnabled: !initial.coverSourcesState[steamRow.policyKey],
      });

      expect(isCoverSourceSaving(result.state, steamRow.settingKey)).toBe(true);
      expect(isCoverSourceSaving(result.state, gogRow.settingKey)).toBe(true);
    });
  });
});
