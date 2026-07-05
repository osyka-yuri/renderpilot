/**
 * Which add-on tool a `blocked_by_other_addon` outcome names (`AddonKind`).
 * Every tool is mutually exclusive with every other tool, per game.
 *
 * Pure cross-cutting taxonomy. Moved out of entities/addon to avoid
 * entity-to-entity coupling for a simple vocabulary type.
 */
export type AddonKind = 'renodx';

export const ALL_ADDON_KINDS: readonly AddonKind[] = ['renodx'];

/** Short display name for an add-on tool, shared by filter chips and any UI
 * copy that names one tool from another's context (e.g. a blocked-by message). */
export const ADDON_DISPLAY_NAME: Record<AddonKind, string> = {
  renodx: 'RenoDX',
};
