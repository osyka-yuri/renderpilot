import {
  loadSteamGridDbKey,
  saveSteamGridDbKey,
  type SteamKeyControllerContext,
} from '@features/settings-steam-key';
import {
  loadCoverRemoteSources,
  persistCoverSourceToggle,
  createInitialSettingsArtworkState,
  isCoverSourceDisabled,
  coverSourceToggleRows,
  type ArtworkControllerContext,
  type CoverSourceToggleRow,
  type SettingsArtworkState,
} from '@features/settings-artwork';
import { getCatalogSetting, setCatalogSetting, type SettingsMessageKind } from '@entities/settings';
import { createDisposableRequestChannel } from '@shared/requests';

export type SettingsPanelModel = ReturnType<typeof createSettingsPanelModel>;

export function createSettingsPanelModel() {
  let disposed = $state(true);

  let steamGridDbKeyInput = $state('');
  let steamKeyLoaded = $state(false);
  let steamKeyBusy = $state(false);
  let steamKeyMessage = $state('');
  let steamKeyMessageKind = $state<SettingsMessageKind | null>(null);

  let artworkState = $state<SettingsArtworkState>(createInitialSettingsArtworkState());
  let coverSourcesMessage = $state('');
  let coverSourcesMessageKind = $state<SettingsMessageKind | null>(null);

  const steamKeyRequest = createDisposableRequestChannel(() => disposed);
  const artworkRequest = createDisposableRequestChannel(() => disposed);

  const steamKeyContext: SteamKeyControllerContext = {
    request: steamKeyRequest,
    getCatalogSetting,
    setCatalogSetting,
    state: {
      readInput: () => steamGridDbKeyInput,
      writeInput: (value: string) => {
        steamGridDbKeyInput = value;
      },
      setBusy: (value: boolean) => {
        steamKeyBusy = value;
      },
      setLoaded: (value: boolean) => {
        steamKeyLoaded = value;
      },
      setMessage: (value: string, kind?: SettingsMessageKind) => {
        steamKeyMessage = value;
        steamKeyMessageKind = kind ?? null;
      },
    },
  };

  const artworkContext: ArtworkControllerContext = {
    request: artworkRequest,
    getCatalogSetting,
    setCatalogSetting,
    state: {
      readState: () => artworkState,
      writeState: (nextState: SettingsArtworkState) => {
        artworkState = nextState;
      },
      setMessage: (value: string, kind?: SettingsMessageKind) => {
        coverSourcesMessage = value;
        coverSourcesMessageKind = kind ?? null;
      },
    },
  };

  function init(): void {
    disposed = false;
    loadInitialSettings();
  }

  function dispose(): void {
    disposed = true;
  }

  function loadInitialSettings(): void {
    void Promise.all([loadSteamGridDbKey(steamKeyContext), loadCoverRemoteSources(artworkContext)]);
  }

  function canSaveSteamGridDbKey(): boolean {
    return steamKeyLoaded && !steamKeyBusy;
  }

  function handleSteamGridDbKeySave(): void {
    if (!canSaveSteamGridDbKey()) {
      return;
    }

    void saveSteamGridDbKey(steamKeyContext);
  }

  function isCoverSourceDisabledState(row: CoverSourceToggleRow): boolean {
    return isCoverSourceDisabled(artworkState, row);
  }

  function handleCoverSourceToggle(row: CoverSourceToggleRow): void {
    if (isCoverSourceDisabledState(row)) {
      return;
    }

    const previousEnabled = artworkState.coverSourcesState[row.policyKey];

    void persistCoverSourceToggle(artworkContext, row, !previousEnabled, previousEnabled);
  }

  return {
    get steamGridDbKeyInput() {
      return steamGridDbKeyInput;
    },
    set steamGridDbKeyInput(value: string) {
      steamGridDbKeyInput = value;
    },
    get steamKeyLoaded() {
      return steamKeyLoaded;
    },
    get steamKeyBusy() {
      return steamKeyBusy;
    },
    get steamKeyMessage() {
      return steamKeyMessage;
    },
    get steamKeyMessageKind() {
      return steamKeyMessageKind;
    },
    get coverSourcesState() {
      return artworkState.coverSourcesState;
    },
    get coverSourcesMessage() {
      return coverSourcesMessage;
    },
    get coverSourcesMessageKind() {
      return coverSourcesMessageKind;
    },

    coverSourceToggleRows,
    init,
    dispose,
    handleSteamGridDbKeySave,
    isCoverSourceDisabled: isCoverSourceDisabledState,
    handleCoverSourceToggle,
  };
}
