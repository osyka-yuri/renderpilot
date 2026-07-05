/**
 * UI-layer types for shared add-on components. Kept as plain `.ts` so they can
 * be re-exported from the barrel without relying on `export type` from `.svelte`
 * (TypeScript cannot re-export those reliably).
 */

import type { ToolI18nPrefix } from '../model/presenters';
import type { MatchConfidence } from '../model/types';

/** Confidence badge vocabulary — same wire contract as {@link MatchConfidence}. */
export type AddonMatchConfidence = MatchConfidence;

/** i18n key prefix for tool-specific freshness / status copy. */
export type AddonToolI18nPrefix = ToolI18nPrefix;
