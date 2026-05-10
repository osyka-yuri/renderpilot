import type { CoverRemotePolicy } from '@shared/covers/cover-sync';
import type {
  CoverSourcePolicyKey,
  CoverSourceSettingKey,
  CoverSourceToggleRow,
} from '@features/settings/settings-screen-model';
import { defaultCoverSourcesState } from '@features/settings/settings-screen-model';

type CoverSourceMutationVersions = Partial<Record<CoverSourceSettingKey, number>>;

export type SettingsArtworkState = {
  coverSourcesState: CoverRemotePolicy;
  coverSourcesLoaded: boolean;
  coverSourcesBusy: boolean;
  savingCoverSourceKeys: ReadonlySet<CoverSourceSettingKey>;
  coverSourceMutationVersion: CoverSourceMutationVersions;
};

function createCoverSourcesPolicy(policy: CoverRemotePolicy): CoverRemotePolicy {
  return {
    steamCdn: policy.steamCdn,
    gogCdn: policy.gogCdn,
    steamgriddb: policy.steamgriddb,
  };
}

function areCoverSourcesPoliciesEqual(left: CoverRemotePolicy, right: CoverRemotePolicy): boolean {
  return (
    left.steamCdn === right.steamCdn &&
    left.gogCdn === right.gogCdn &&
    left.steamgriddb === right.steamgriddb
  );
}

function withCoverSourceMutationVersion(
  state: SettingsArtworkState,
  settingKey: CoverSourceSettingKey,
  version: number,
): SettingsArtworkState {
  return {
    ...state,
    coverSourceMutationVersion: {
      ...state.coverSourceMutationVersion,
      [settingKey]: version,
    },
  };
}

export function createInitialSettingsArtworkState(): SettingsArtworkState {
  return {
    coverSourcesState: createCoverSourcesPolicy(defaultCoverSourcesState),
    coverSourcesLoaded: false,
    coverSourcesBusy: false,
    savingCoverSourceKeys: new Set<CoverSourceSettingKey>(),
    coverSourceMutationVersion: {},
  };
}

export function withCoverSourceValue(
  state: SettingsArtworkState,
  policyKey: CoverSourcePolicyKey,
  enabled: boolean,
): SettingsArtworkState {
  if (state.coverSourcesState[policyKey] === enabled) {
    return state;
  }

  return {
    ...state,
    coverSourcesState: {
      ...state.coverSourcesState,
      [policyKey]: enabled,
    },
  };
}

export function withCoverSourcesBusy(
  state: SettingsArtworkState,
  busy: boolean,
): SettingsArtworkState {
  if (state.coverSourcesBusy === busy) {
    return state;
  }

  return {
    ...state,
    coverSourcesBusy: busy,
  };
}

export function withCoverSourcesLoaded(
  state: SettingsArtworkState,
  loaded: boolean,
): SettingsArtworkState {
  if (state.coverSourcesLoaded === loaded) {
    return state;
  }

  return {
    ...state,
    coverSourcesLoaded: loaded,
  };
}

export function withCoverSourcesPolicy(
  state: SettingsArtworkState,
  policy: CoverRemotePolicy,
): SettingsArtworkState {
  if (areCoverSourcesPoliciesEqual(state.coverSourcesState, policy)) {
    return state;
  }

  return {
    ...state,
    coverSourcesState: createCoverSourcesPolicy(policy),
  };
}

export function withCoverSourceSaving(
  state: SettingsArtworkState,
  settingKey: CoverSourceSettingKey,
  saving: boolean,
): SettingsArtworkState {
  const isSaving = state.savingCoverSourceKeys.has(settingKey);

  if (isSaving === saving) {
    return state;
  }

  const savingCoverSourceKeys = new Set(state.savingCoverSourceKeys);

  if (saving) {
    savingCoverSourceKeys.add(settingKey);
  } else {
    savingCoverSourceKeys.delete(settingKey);
  }

  return {
    ...state,
    savingCoverSourceKeys,
  };
}

export function isCoverSourceSaving(
  state: SettingsArtworkState,
  settingKey: CoverSourceSettingKey,
): boolean {
  return state.savingCoverSourceKeys.has(settingKey);
}

export function isCoverSourceDisabled(
  state: SettingsArtworkState,
  row: CoverSourceToggleRow,
): boolean {
  return (
    !state.coverSourcesLoaded ||
    state.coverSourcesBusy ||
    isCoverSourceSaving(state, row.settingKey)
  );
}

export function nextCoverSourceMutationVersion(
  state: SettingsArtworkState,
  settingKey: CoverSourceSettingKey,
): { state: SettingsArtworkState; version: number } {
  const version = (state.coverSourceMutationVersion[settingKey] ?? 0) + 1;

  return {
    state: withCoverSourceMutationVersion(state, settingKey, version),
    version,
  };
}

export function beginCoverSourceMutation(params: {
  state: SettingsArtworkState;
  row: CoverSourceToggleRow;
  nextEnabled: boolean;
}): { state: SettingsArtworkState; version: number } {
  const { state, row, nextEnabled } = params;

  const versioned = nextCoverSourceMutationVersion(state, row.settingKey);

  return {
    state: withCoverSourceValue(
      withCoverSourceSaving(versioned.state, row.settingKey, true),
      row.policyKey,
      nextEnabled,
    ),
    version: versioned.version,
  };
}

export function isCurrentCoverSourceMutation(
  state: SettingsArtworkState,
  settingKey: CoverSourceSettingKey,
  mutationVersion: number,
): boolean {
  return state.coverSourceMutationVersion[settingKey] === mutationVersion;
}
