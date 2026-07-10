import type { AppUpdateHandle } from '../api/app-updater-gateway';
import { normalizeReleaseDate } from './format-release-date';
import { parseReleaseNotes } from './release-notes';
import type { AppUpdateOffer } from './types';

/** Map a gateway update handle into a serializable, UI-ready offer. */
export function toOffer(handle: AppUpdateHandle): AppUpdateOffer {
  return {
    currentVersion: handle.metadata.currentVersion,
    version: handle.metadata.version,
    date: normalizeReleaseDate(handle.metadata.date),
    releaseNotes: parseReleaseNotes(handle.metadata.body),
  };
}
