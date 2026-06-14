export type {
  BuildType,
  Signature,
  LibraryManifest,
  LibraryManifestEntry,
  LibraryState,
} from './model/types';

export {
  fetchLibrariesManifest,
  getLibrariesManifest,
  downloadLibrary,
  downloadArtifact,
  deleteLibrary,
  getLibraryStates,
} from './api/desktop';

export { clearDownloadProgress, sumDownloadFractions } from './model/download-progress.svelte';
export type { DownloadProgress } from './model/download-progress.svelte';
export { default as DownloadProgressBar } from './ui/DownloadProgressBar.svelte';
export { default as BatchDownloadProgressBar } from './ui/BatchDownloadProgressBar.svelte';
