import { publishStatusNotification } from '@shared/notifications';

const STALE_PLAN_DESCRIPTION =
  'The selected operation plan is no longer current. Rebuild the plan before applying it.';

const MISSING_STABLE_GAME_ID_DESCRIPTION =
  'Catalog returned game details without a stable identifier.';

export function publishStalePlanNotification(): string {
  return publishStatusNotification(STALE_PLAN_DESCRIPTION, 'error') ?? 'desktop-status';
}

export function publishMissingStableGameDetailsNotification(): string {
  return publishStatusNotification(MISSING_STABLE_GAME_ID_DESCRIPTION, 'error') ?? 'desktop-status';
}