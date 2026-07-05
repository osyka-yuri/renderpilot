import type { AddonStoreView } from './store-view';

/** The subset of store state every tool's `getCardView` reads, before its own
 * outcome-specific views. */
export type CardViewSource = Pick<
  AddonStoreView,
  | 'loading'
  | 'loaded'
  | 'loadError'
  | 'isInstalled'
  | 'isBlockedByOtherAddon'
  | 'isBlacklisted'
  | 'isUnsupported'
  | 'isIncompatible'
  | 'isInstallable'
>;

/** Views every tool's card can resolve to, before its own outcome-specific
 * views (for example RenoDX: `external` | `native-hdr`). */
export type BaseCardView =
  | 'loading'
  | 'load-error'
  | 'installed'
  | 'blocked-by-other-addon'
  | 'blacklisted'
  | 'unsupported'
  | 'incompatible'
  | 'installable'
  | 'unavailable';

/**
 * Resolves which top-level view an add-on card renders. Order is priority,
 * highest first: a load in progress or a load error always wins over any
 * availability outcome. `resolveOutcomeView` is consulted right after
 * `blocked-by-other-addon` and before `blacklisted`, for the tool-specific
 * outcome kinds every backend otherwise reports as mutually exclusive.
 */
export function getCardView<TOutcomeView extends string>(
  source: CardViewSource,
  resolveOutcomeView: () => TOutcomeView | null,
): BaseCardView | TOutcomeView {
  if (source.loading && !source.loaded) {
    return 'loading';
  }
  if (source.loadError) {
    return 'load-error';
  }
  if (source.isInstalled) {
    return 'installed';
  }
  if (source.isBlockedByOtherAddon) {
    return 'blocked-by-other-addon';
  }

  const outcomeView = resolveOutcomeView();
  if (outcomeView) {
    return outcomeView;
  }

  if (source.isBlacklisted) {
    return 'blacklisted';
  }
  if (source.isUnsupported) {
    return 'unsupported';
  }
  if (source.isIncompatible) {
    return 'incompatible';
  }
  if (source.isInstallable) {
    return 'installable';
  }
  return 'unavailable';
}
