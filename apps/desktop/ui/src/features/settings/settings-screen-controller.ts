import type { CatalogSettingPayload } from '@shared/api/types';
import type { CoverRemotePolicy } from '@shared/covers/cover-sync';
import { fetchCoverRemotePolicy } from '@shared/covers/cover-sync';
import type { DisposableRequestChannel } from '@shared/utils/request-channel';
import type {
  CoverSourceToggleRow,
  CoverSourceSettingKey,
} from '@features/settings/settings-screen-model';
import {
  artworkSettingsReadError,
  artworkSourceSaveError,
  catalogReadError,
  formatBooleanSetting,
  steamGridDbSettingKey,
  steamKeySaveError,
} from '@features/settings/settings-screen-model';
import {
  beginCoverSourceMutation,
  isCurrentCoverSourceMutation,
  withCoverSourcesBusy,
  withCoverSourcesLoaded,
  withCoverSourcesPolicy,
  withCoverSourceSaving,
  withCoverSourceValue,
  type SettingsArtworkState,
} from '@features/settings/settings-screen-state';

type GetCatalogSetting = (key: string) => Promise<CatalogSettingPayload>;
type SetCatalogSetting = (key: string, value: string) => Promise<unknown>;

type FetchCoverRemotePolicy = (getCatalogSetting: GetCatalogSetting) => Promise<CoverRemotePolicy>;

type SteamKeyStateChannel = {
  readInput: () => string;
  writeInput: (value: string) => void;
  setBusy: (busy: boolean) => void;
  setLoaded: (loaded: boolean) => void;
  setMessage: (message: string) => void;
};

type ArtworkStateChannel = {
  readState: () => SettingsArtworkState;
  writeState: (nextState: SettingsArtworkState) => void;
  setMessage: (message: string) => void;
};

export type SteamKeyControllerContext = {
  request: DisposableRequestChannel;
  getCatalogSetting: GetCatalogSetting;
  setCatalogSetting: SetCatalogSetting;
  state: SteamKeyStateChannel;
};

export type ArtworkControllerContext = {
  request: DisposableRequestChannel;
  getCatalogSetting: GetCatalogSetting;
  setCatalogSetting: SetCatalogSetting;
  state: ArtworkStateChannel;
  fetchCoverRemotePolicy?: FetchCoverRemotePolicy;
};

const steamKeySavedMessage = 'Saved.';
const steamKeyClearedMessage = 'Key cleared.';

export async function loadSteamGridDbKey(context: SteamKeyControllerContext): Promise<void> {
  const requestId = beginWritableRequest(context.request);

  if (requestId === null) {
    return;
  }

  context.state.setBusy(true);
  context.state.setMessage('');

  try {
    const payload = await context.getCatalogSetting(steamGridDbSettingKey);

    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.writeInput(payload.value ?? '');
      context.state.setLoaded(true);
    }
  } catch {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setMessage(catalogReadError);
      context.state.setLoaded(true);
    }
  } finally {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setBusy(false);
    }
  }
}

export async function saveSteamGridDbKey(context: SteamKeyControllerContext): Promise<void> {
  const requestId = beginWritableRequest(context.request);

  if (requestId === null) {
    return;
  }

  const normalizedKey = context.state.readInput().trim();

  context.state.setBusy(true);
  context.state.setMessage('');

  try {
    await context.setCatalogSetting(steamGridDbSettingKey, normalizedKey);

    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.writeInput(normalizedKey);
      context.state.setMessage(normalizedKey ? steamKeySavedMessage : steamKeyClearedMessage);
    }
  } catch {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setMessage(steamKeySaveError);
    }
  } finally {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setBusy(false);
    }
  }
}

export async function loadCoverRemoteSources(context: ArtworkControllerContext): Promise<void> {
  const requestId = beginWritableRequest(context.request);

  if (requestId === null) {
    return;
  }

  updateArtworkState(context, (state) => withCoverSourcesBusy(state, true));
  context.state.setMessage('');

  try {
    const policy = await readCoverRemotePolicy(context);

    if (canWriteLatestRequest(context.request, requestId)) {
      updateArtworkState(context, (state) =>
        withCoverSourcesPolicy(state, normalizeCoverRemotePolicy(policy)),
      );
    }
  } catch {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setMessage(artworkSettingsReadError);
    }
  } finally {
    if (canWriteLatestRequest(context.request, requestId)) {
      updateArtworkState(context, (state) => {
        const loadedState = withCoverSourcesLoaded(state, true);
        return withCoverSourcesBusy(loadedState, false);
      });
    }
  }
}

export async function persistCoverSourceToggle(
  context: ArtworkControllerContext,
  row: CoverSourceToggleRow,
  nextEnabled: boolean,
  previousEnabled: boolean,
): Promise<void> {
  if (context.request.isDisposed()) {
    return;
  }

  const mutation = beginCoverSourceMutation({
    state: context.state.readState(),
    row,
    nextEnabled,
  });

  context.state.writeState(mutation.state);
  context.state.setMessage('');

  try {
    await context.setCatalogSetting(row.settingKey, formatBooleanSetting(nextEnabled));
  } catch {
    if (isCurrentArtworkMutation(context, row.settingKey, mutation.version)) {
      updateArtworkState(context, (state) =>
        withCoverSourceValue(state, row.policyKey, previousEnabled),
      );

      context.state.setMessage(artworkSourceSaveError);
    }
  } finally {
    if (isCurrentArtworkMutation(context, row.settingKey, mutation.version)) {
      updateArtworkState(context, (state) => withCoverSourceSaving(state, row.settingKey, false));
    }
  }
}

function beginWritableRequest(request: DisposableRequestChannel): number | null {
  if (request.isDisposed()) {
    return null;
  }

  const requestId = request.begin();

  if (request.isDisposed()) {
    return null;
  }

  return requestId;
}

function canWriteLatestRequest(request: DisposableRequestChannel, requestId: number): boolean {
  return !request.isDisposed() && request.isActive(requestId);
}

function isCurrentArtworkMutation(
  context: ArtworkControllerContext,
  settingKey: CoverSourceSettingKey,
  mutationVersion: number,
): boolean {
  return (
    !context.request.isDisposed() &&
    isCurrentCoverSourceMutation(context.state.readState(), settingKey, mutationVersion)
  );
}

function updateArtworkState(
  context: ArtworkControllerContext,
  getNextState: (state: SettingsArtworkState) => SettingsArtworkState,
): void {
  context.state.writeState(getNextState(context.state.readState()));
}

function readCoverRemotePolicy(context: ArtworkControllerContext): Promise<CoverRemotePolicy> {
  const readPolicy = context.fetchCoverRemotePolicy ?? fetchCoverRemotePolicy;
  return readPolicy(context.getCatalogSetting);
}

function normalizeCoverRemotePolicy(policy: CoverRemotePolicy): CoverRemotePolicy {
  return {
    steamCdn: policy.steamCdn,
    gogCdn: policy.gogCdn,
    steamgriddb: policy.steamgriddb,
  };
}
