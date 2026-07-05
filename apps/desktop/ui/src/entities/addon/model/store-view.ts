import type {
  Freshness,
  HostActions,
  HostDetection,
  HostFacts,
  MatchConfidence,
  UpdateStatus,
} from './types';

/**
 * The shared, tool-agnostic slice of an add-on store that the shared UI
 * (card view, installable view, installed panel) reads. Every tool's
 * concrete store structurally satisfies this — the fields come from
 * {@link addonCoreApi}, {@link commonOutcomeApi}, and {@link hostSnapshotApi}.
 * Each consuming component picks only the fields it needs.
 */
export type AddonStoreView = {
  loading: boolean;
  loaded: boolean;
  loadError: string | null;
  busy: boolean;
  isInstalled: boolean;
  isInstallable: boolean;
  isBlockedByOtherAddon: boolean;
  isBlacklisted: boolean;
  isUnsupported: boolean;
  isIncompatible: boolean;
  requiresConfirmation: boolean;
  confidence: MatchConfidence | null;
  notesKeys: string[];
  freshness: Freshness;
  addonDated: string | null;
  installedAt: number | null;
  lastCheckedAt: number | null;
  updateAvailable: boolean;
  hostUpdate: UpdateStatus | null;
  addonUpdate: UpdateStatus | null;
  hostDetection: HostDetection;
  hostFacts: HostFacts;
  hostActions: HostActions;
  checkForUpdates(gameId: string): Promise<unknown>;
  update(gameId: string): Promise<unknown>;
  uninstall(gameId: string): Promise<boolean | undefined>;
};
