import { applySwap, type ApplySwapResult } from '@entities/operation';
import { downloadLibrary, type LibraryState } from '@entities/library';

export type ExecuteGraphicsSwapInput = {
  gameId: string;
  componentId: string;
  artifactId: string;
  entryId?: string | null;
  signal?: AbortSignal;
};

export type ExecuteGraphicsSwapDeps = {
  applySwap?: typeof applySwap;
  downloadLibrary?: (entryId: string) => Promise<LibraryState>;
};

export async function executeGraphicsSwap(
  input: ExecuteGraphicsSwapInput,
  deps: ExecuteGraphicsSwapDeps = {},
): Promise<ApplySwapResult | null> {
  const resolvedDeps = resolveDeps(deps);
  const artifactId = await resolveSwapArtifactId(
    input.artifactId,
    input.entryId ?? null,
    resolvedDeps.downloadLibrary,
  );

  if (input.signal?.aborted) {
    return null;
  }

  return resolvedDeps.applySwap(input.gameId, input.componentId, artifactId);
}

async function resolveSwapArtifactId(
  artifactId: string,
  entryId: string | null,
  downloadLibrary: (entryId: string) => Promise<LibraryState>,
): Promise<string> {
  if (!entryId) {
    return artifactId;
  }

  const libraryState = await downloadLibrary(entryId);

  if (!libraryState.artifact_id) {
    throw new Error('Downloaded library did not return an artifact id');
  }

  return libraryState.artifact_id;
}

function resolveDeps(deps: ExecuteGraphicsSwapDeps): Required<ExecuteGraphicsSwapDeps> {
  return {
    applySwap: deps.applySwap ?? applySwap,
    downloadLibrary: deps.downloadLibrary ?? downloadLibrary,
  };
}
