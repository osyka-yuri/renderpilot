/**
 * Pure (non-Svelte) status vocabulary and presentation data for add-on badges.
 * Extracted from the .svelte components so that typing is reliable and
 * eslint/svelte-check do not report "unsafe assignment" on prop passing.
 *
 * This module contains only data + types — no components.
 */

export type AddonBadgeStatus =
  | 'current'
  | 'available'
  | 'unknown'
  | 'untracked'
  | 'checking'
  | 'channel_mismatch'
  | 'unknown_needs_validation';

export type StatusIcon = 'success' | 'update' | 'checking' | 'info';

const MUTED_TINT = 'text-muted-foreground';
const SUCCESS_TINT = 'border-transparent bg-emerald-500/10 text-emerald-600 dark:text-emerald-400';
const WARNING_TINT = 'border-transparent bg-warning/10 text-warning';

export const ICON_BY_STATUS = {
  current: 'success',
  available: 'update',
  unknown: 'info',
  untracked: 'info',
  checking: 'checking',
  channel_mismatch: 'update',
  unknown_needs_validation: 'info',
} satisfies Record<AddonBadgeStatus, StatusIcon>;

export const TINT_BY_STATUS = {
  current: SUCCESS_TINT,
  available: WARNING_TINT,
  unknown: MUTED_TINT,
  untracked: MUTED_TINT,
  checking: MUTED_TINT,
  channel_mismatch: WARNING_TINT,
  unknown_needs_validation: MUTED_TINT,
} satisfies Record<AddonBadgeStatus, string>;
