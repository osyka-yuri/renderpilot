import { parseRfc3339Timestamp } from '@shared/date';
import { parseReleaseNotes } from '@shared/model';

import type { AppUpdateHandle } from '../api/app-updater-gateway';
import { selectUpdateReleaseNotes } from './release-notes-range';
import type { AppUpdateOffer } from './types';

/** Map a gateway update handle into a serializable, UI-ready offer. */
export function toOffer(handle: AppUpdateHandle): AppUpdateOffer {
  return {
    currentVersion: handle.metadata.currentVersion,
    version: handle.metadata.version,
    releaseTimestamp: parseRfc3339Timestamp(handle.metadata.date),
    releaseNotes: parseReleaseNotes(
      selectUpdateReleaseNotes(
        handle.metadata.body,
        handle.metadata.currentVersion,
        handle.metadata.version,
      ),
    ),
  };
}
