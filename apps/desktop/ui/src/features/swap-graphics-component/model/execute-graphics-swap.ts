import { applySwap, type ApplySwapResult } from '@entities/operation';
import { downloadArtifact, type LibraryState } from '@entities/library';

export type ExecuteGraphicsSwapInput = {
  gameId: string;
  componentId: string;
  artifactId: string;
  /** Whether the artifact is already downloaded locally; if not, it is fetched first. */
  isDownloaded: boolean;
  signal?: AbortSignal;
};

export type ExecuteGraphicsSwapDeps = {
  applySwap?: typeof applySwap;
  downloadArtifact?: (artifactId: string) => Promise<LibraryState>;
};

export async function executeGraphicsSwap(
  input: ExecuteGraphicsSwapInput,
  deps: ExecuteGraphicsSwapDeps = {},
): Promise<ApplySwapResult | null> {
  const resolvedDeps = resolveDeps(deps);
  const artifactId = await ensureArtifactDownloaded(
    input.artifactId,
    input.isDownloaded,
    resolvedDeps.downloadArtifact,
  );

  if (input.signal?.aborted) {
    return null;
  }

  return resolvedDeps.applySwap(input.gameId, input.componentId, artifactId);
}

/**
 * Returns the artifact id ready to apply, downloading it first when it is not yet
 * local. Download keys on the artifact id, so a single manifest DLL and a composed
 * FSR release package resolve through the same path.
 */
async function ensureArtifactDownloaded(
  artifactId: string,
  isDownloaded: boolean,
  downloadArtifact: (artifactId: string) => Promise<LibraryState>,
): Promise<string> {
  if (isDownloaded) {
    return artifactId;
  }

  const libraryState = await downloadArtifact(artifactId);

  if (!libraryState.artifact_id) {
    throw new Error('Downloaded artifact did not return an artifact id');
  }

  return libraryState.artifact_id;
}

function resolveDeps(deps: ExecuteGraphicsSwapDeps): Required<ExecuteGraphicsSwapDeps> {
  return {
    applySwap: deps.applySwap ?? applySwap,
    downloadArtifact: deps.downloadArtifact ?? downloadArtifact,
  };
}
