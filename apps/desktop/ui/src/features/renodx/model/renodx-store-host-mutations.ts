import { requestMutationSafetyTokens } from '@entities/addon';
import type {
  AddonMutationResult,
  MutationSafetyCapture,
  MutationSafetyScope,
  ReshadeChannel,
  MutationSafetyTokens,
  createAddonStore,
} from '@entities/addon';
import { isFileSafetyContextError } from '@shared/errors';
import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishPresentedErrorNotification } from '@shared/notifications';

import type { RenoDxApi } from '../api/desktop';
import type { AvailabilitySnapshot } from './renodx-store-helpers';
import type {
  AvailabilityOutcome,
  AvailabilityReport,
  HostKind,
  RenoDxInstallState,
  RenoDxUpdateReport,
} from './types';

type RenoDxCore = Pick<
  ReturnType<typeof createAddonStore<RenoDxInstallState, RenoDxUpdateReport, AvailabilityReport>>,
  'runBusyMutation' | 'busy' | 'state' | 'updateAvailable' | 'safetyContextError'
>;

export type RenoDxHostMutationOptions = {
  api: Pick<RenoDxApi, 'install' | 'installFromFile' | 'update' | 'switchChannel' | 'uninstall'>;
  core: RenoDxCore;
  getAvailabilitySnapshot: () => AvailabilitySnapshot;
  getOutcome: () => AvailabilityOutcome | null;
  getManualInstallHostKind: () => HostKind | null;
  onChannelSwitched: (channel: ReshadeChannel) => void;
  channelIsSupported: (channel: ReshadeChannel) => boolean;
  requireSafetyTokens?: (
    gameId: string,
    scope: 'game' | 'game_and_shared',
  ) => Promise<MutationSafetyTokens | null>;
  afterInstallLikeCommit: (
    gameId: string,
    token: number,
    channel?: ReshadeChannel,
  ) => void | Promise<void>;
  afterCapabilityCommit: (
    gameId: string,
    token: number,
    channel?: ReshadeChannel,
  ) => void | Promise<void>;
};

function safetyScopeForHost(hostKind: HostKind | null | undefined): MutationSafetyScope {
  return hostKind === 'proxy' ? 'game' : 'game_and_shared';
}

function safetyScopeForInstall(
  hostKind: HostKind | null | undefined,
  existingSharedOwner: boolean,
): MutationSafetyScope {
  return existingSharedOwner ? 'game_and_shared' : safetyScopeForHost(hostKind);
}

function plannedInstallHostKind(
  outcome: AvailabilityOutcome | null,
  manualInstallHost: HostKind | null,
): HostKind | null {
  if (outcome?.kind === 'installable') {
    return outcome.host_kind;
  }
  if (outcome?.kind === 'external' && outcome.file_install) {
    return outcome.file_install.host_kind;
  }
  return manualInstallHost;
}

/** Host and companion mutation entry points for the RenoDX card. */
export function createRenoDxHostMutations(options: RenoDxHostMutationOptions) {
  const {
    api,
    core,
    getAvailabilitySnapshot,
    getOutcome,
    getManualInstallHostKind,
    onChannelSwitched,
    channelIsSupported,
    requireSafetyTokens,
    afterInstallLikeCommit,
    afterCapabilityCommit,
  } = options;

  let updateSafetyContextOutcome: { value: unknown } | null = null;
  let updateCaptureGeneration = 0;

  function captureSafetyTokens(
    gameId: string,
    scope: MutationSafetyScope,
  ): Promise<MutationSafetyCapture> {
    if (core.busy) {
      return Promise.resolve({ kind: 'cancelled' });
    }

    return requestMutationSafetyTokens(requireSafetyTokens, gameId, scope);
  }

  function handleSafetyCaptureError(
    error: unknown,
    errorKey: MessageKeyWithoutParams,
    onSafetyContextError?: (error: unknown) => void,
  ): AddonMutationResult {
    if (isFileSafetyContextError(error)) {
      onSafetyContextError?.(error);
    }
    publishPresentedErrorNotification(t(errorKey), error);
    return 'failed';
  }

  function runWithSafetyTokens(
    gameId: string,
    scope: MutationSafetyScope,
    errorKey: MessageKeyWithoutParams,
    run: (tokens?: MutationSafetyTokens) => Promise<AddonMutationResult>,
    onSafetyContextError?: (error: unknown) => void,
  ): Promise<AddonMutationResult> {
    if (!requireSafetyTokens) {
      // Preserve the core mutation's same-turn busy claim when no gate exists.
      return run();
    }

    return captureSafetyTokens(gameId, scope).then(
      (safety) => (safety.kind === 'cancelled' ? 'skipped' : run(safety.tokens)),
      (error: unknown) => handleSafetyCaptureError(error, errorKey, onSafetyContextError),
    );
  }

  function installedHostKind(): HostKind | null {
    return core.state?.status === 'installed' ? core.state.host_kind : null;
  }

  async function install(gameId: string, channel: ReshadeChannel): Promise<AddonMutationResult> {
    if (!channelIsSupported(channel)) {
      return 'skipped';
    }
    const snapshot = getAvailabilitySnapshot();
    const safetyScope = safetyScopeForInstall(
      plannedInstallHostKind(getOutcome(), getManualInstallHostKind()),
      snapshot.installRequiresSharedVulkan,
    );
    return runWithSafetyTokens(gameId, safetyScope, 'gameDetails.renodx.installError', (tokens) =>
      core.runBusyMutation(
        gameId,
        () =>
          tokens
            ? api.install(gameId, channel, tokens.gameContextToken, tokens.sharedVulkanContextToken)
            : api.install(gameId, channel),
        {
          errorKey: 'gameDetails.renodx.installError',
          safetyScope,
          afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
          notifyExclusivity: true,
        },
      ),
    );
  }

  async function installFromFile(
    gameId: string,
    filePath: string,
    channel: ReshadeChannel,
  ): Promise<AddonMutationResult> {
    if (!channelIsSupported(channel)) {
      return 'skipped';
    }
    const snapshot = getAvailabilitySnapshot();
    const safetyScope = safetyScopeForInstall(
      plannedInstallHostKind(getOutcome(), getManualInstallHostKind()),
      snapshot.installRequiresSharedVulkan,
    );
    return runWithSafetyTokens(gameId, safetyScope, 'gameDetails.renodx.installError', (tokens) =>
      core.runBusyMutation(
        gameId,
        () =>
          tokens
            ? api.installFromFile(
                gameId,
                filePath,
                channel,
                tokens.gameContextToken,
                tokens.sharedVulkanContextToken,
              )
            : api.installFromFile(gameId, filePath, channel),
        {
          errorKey: 'gameDetails.renodx.installError',
          safetyScope,
          afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
          notifyExclusivity: true,
        },
      ),
    );
  }

  async function update(gameId: string): Promise<AddonMutationResult> {
    if (core.busy || !core.updateAvailable) {
      return 'skipped';
    }
    const captureGeneration = ++updateCaptureGeneration;
    updateSafetyContextOutcome = { value: null };
    const safetyScope = safetyScopeForHost(installedHostKind());
    return runWithSafetyTokens(
      gameId,
      safetyScope,
      'gameDetails.renodx.updateError',
      (tokens) =>
        core
          .runBusyMutation(
            gameId,
            () =>
              tokens
                ? api.update(gameId, tokens.gameContextToken, tokens.sharedVulkanContextToken)
                : api.update(gameId),
            {
              errorKey: 'gameDetails.renodx.updateError',
              safetyScope,
              requireUpdateAvailable: true,
              afterCommit: (token) => afterInstallLikeCommit(gameId, token),
            },
          )
          .finally(() => {
            if (captureGeneration === updateCaptureGeneration) {
              const coreError = core.safetyContextError;
              updateSafetyContextOutcome = {
                value: isFileSafetyContextError(coreError) ? coreError : null,
              };
            }
          }),
      (failure) => {
        if (captureGeneration === updateCaptureGeneration) {
          updateSafetyContextOutcome = { value: failure };
        }
      },
    );
  }

  async function switchChannel(
    gameId: string,
    channel: ReshadeChannel,
  ): Promise<AddonMutationResult> {
    const snapshot = getAvailabilitySnapshot();
    const action = snapshot.actions.switch_channel;
    if (
      core.busy ||
      core.state?.status !== 'installed' ||
      core.state.host_kind !== 'proxy' ||
      action?.enabled !== true ||
      action.target_channel !== channel ||
      channel === snapshot.hostFacts.channel.detected
    ) {
      return 'skipped';
    }
    const safetyScope = 'game' satisfies MutationSafetyScope;
    return runWithSafetyTokens(gameId, safetyScope, 'gameDetails.renodx.switchError', (tokens) =>
      core.runBusyMutation(
        gameId,
        () =>
          tokens
            ? api.switchChannel(
                gameId,
                channel,
                tokens.gameContextToken,
                tokens.sharedVulkanContextToken,
              )
            : api.switchChannel(gameId, channel),
        {
          errorKey: 'gameDetails.renodx.switchError',
          safetyScope,
          afterCommit: () => {
            onChannelSwitched(channel);
          },
        },
      ),
    );
  }

  async function uninstall(gameId: string): Promise<AddonMutationResult> {
    return core.runBusyMutation(gameId, () => api.uninstall(gameId), {
      errorKey: 'gameDetails.renodx.uninstallError',
      clearDownloadProgress: false,
      notifyExclusivity: true,
      afterCommit: (token) => afterCapabilityCommit(gameId, token),
    });
  }

  return {
    install,
    installFromFile,
    update,
    switchChannel,
    uninstall,
    get safetyContextError() {
      return updateSafetyContextOutcome?.value;
    },
  };
}
