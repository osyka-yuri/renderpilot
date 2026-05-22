import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import type { LibraryManifest, LibraryState } from '../model/types';

export async function fetchLibrariesManifest(): Promise<LibraryManifest> {
  return invokeDesktop<LibraryManifest>('fetch_libraries_manifest');
}

export async function getLibrariesManifest(): Promise<LibraryManifest> {
  return invokeDesktop<LibraryManifest>('get_libraries_manifest');
}

export async function downloadLibrary(entryId: string): Promise<LibraryState> {
  return invokeDesktop<LibraryState>('download_library', {
    entryId: requireNonBlankString(entryId, 'entryId'),
  });
}

export async function deleteLibrary(entryId: string): Promise<LibraryState> {
  return invokeDesktop<LibraryState>('delete_library', {
    entryId: requireNonBlankString(entryId, 'entryId'),
  });
}

export async function getLibraryStates(): Promise<LibraryState[]> {
  return invokeDesktop<LibraryState[]>('get_library_states');
}
