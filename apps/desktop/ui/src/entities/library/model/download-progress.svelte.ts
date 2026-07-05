// FSD boundary shim: canonical implementation lives in `@shared/lib/download-progress`.
// Library UI re-exports these symbols so pages can import progress helpers next to
// `DownloadProgressBar` from `@entities/library`. Do not re-implement the map here.
import {
  clearDownloadProgress as _clear,
  DOWNLOAD_PROGRESS_EVENT as _EVENT,
  latestDownloadProgress as _latest,
  sumDownloadFractions as _sum,
  type DownloadProgress as _DP,
} from '@shared/lib';

export const clearDownloadProgress = _clear;
export const DOWNLOAD_PROGRESS_EVENT = _EVENT;
export const latestDownloadProgress = _latest;
export const sumDownloadFractions = _sum;
export type DownloadProgress = _DP;
