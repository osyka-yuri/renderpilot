import { parseRfc3339Timestamp } from '@shared/date';

import type { AppUpdateHandle } from '../api/app-updater-gateway';
import { parseReleaseNotes } from './release-notes';
import type { AppUpdateOffer } from './types';

/** Map a gateway update handle into a serializable, UI-ready offer. */
export function toOffer(handle: AppUpdateHandle): AppUpdateOffer {
  return {
    currentVersion: handle.metadata.currentVersion,
    version: handle.metadata.version,
    releaseTimestamp: parseRfc3339Timestamp(handle.metadata.date),
    releaseNotes: parseReleaseNotes(handle.metadata.body),
  };
}
