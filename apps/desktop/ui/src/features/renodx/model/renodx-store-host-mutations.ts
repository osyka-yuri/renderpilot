import type {
  AddonMutationResult,
  MutationSafetyScope,
  ReshadeChannel,
  MutationSafetyTokens,
  createAddonStore,
} from '@entities/addon';

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
  'runBusyMutation' | 'busy' | 'state'
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
  ) => Promise<MutationSafetyTokens>;
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

  function installedHostKind(): HostKind | null {
    return core.state?.status === 'installed' ? core.state.host_kind : null;
  }

  async function install(gameId: string, channel: ReshadeChannel): Promise<AddonMutationResult> {
    if (!channelIsSupported(channel)) {
      return 'skipped';
    }
    const safetyScope = safetyScopeForHost(
      plannedInstallHostKind(getOutcome(), getManualInstallHostKind()),
    );
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, safetyScope);
        return tokens
          ? api.install(gameId, channel, tokens.gameContextToken, tokens.sharedVulkanContextToken)
          : api.install(gameId, channel);
      },
      {
        errorKey: 'gameDetails.renodx.installError',
        safetyScope,
        afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
        notifyExclusivity: true,
      },
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
    const safetyScope = safetyScopeForHost(
      plannedInstallHostKind(getOutcome(), getManualInstallHostKind()),
    );
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, safetyScope);
        return tokens
          ? api.installFromFile(
              gameId,
              filePath,
              channel,
              tokens.gameContextToken,
              tokens.sharedVulkanContextToken,
            )
          : api.installFromFile(gameId, filePath, channel);
      },
      {
        errorKey: 'gameDetails.renodx.installError',
        safetyScope,
        afterCommit: (token) => afterCapabilityCommit(gameId, token, channel),
        notifyExclusivity: true,
      },
    );
  }

  async function update(gameId: string): Promise<AddonMutationResult> {
    const safetyScope = safetyScopeForHost(installedHostKind());
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, safetyScope);
        return tokens
          ? api.update(gameId, tokens.gameContextToken, tokens.sharedVulkanContextToken)
          : api.update(gameId);
      },
      {
        errorKey: 'gameDetails.renodx.updateError',
        safetyScope,
        requireUpdateAvailable: true,
        afterCommit: (token) => afterInstallLikeCommit(gameId, token),
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
    return core.runBusyMutation(
      gameId,
      async () => {
        const tokens = await requireSafetyTokens?.(gameId, safetyScope);
        return tokens
          ? api.switchChannel(
              gameId,
              channel,
              tokens.gameContextToken,
              tokens.sharedVulkanContextToken,
            )
          : api.switchChannel(gameId, channel);
      },
      {
        errorKey: 'gameDetails.renodx.switchError',
        safetyScope,
        afterCommit: () => {
          onChannelSwitched(channel);
        },
      },
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
  };
}
