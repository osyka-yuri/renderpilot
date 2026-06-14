import type { GameCandidateGroup, GameDetails, GameGraphicsComponent } from '@entities/game';

/**
 * Shared test factories for game-details candidate groups. Imported only by
 * `*.test.ts` files (never by app code), so it stays out of the production
 * bundle and out of the vitest `*.test.ts` suite glob.
 */

export type Candidate = GameCandidateGroup['candidates'][number];

export function component(id: string, technology = 'nvidia_streamline'): GameGraphicsComponent {
  return {
    id,
    game_id: 'game-1',
    kind: 'native_library',
    technology,
    swappability: technology === 'nvidia_streamline' ? 'bundle_only' : 'swappable',
    files: [{ path: `C:/Game/${id}.dll` }],
    rollback_available: false,
  };
}

export function candidate(version: string | null, overrides: Partial<Candidate> = {}): Candidate {
  return {
    artifact_id: overrides.artifact_id ?? `artifact:${version}`,
    file_name: 'lib.dll',
    file_path: null,
    version,
    source_game_id: null,
    comparison: overrides.comparison ?? 'newer_version',
    manifest_entry_id: overrides.manifest_entry_id ?? null,
    is_downloaded: overrides.is_downloaded ?? true,
    is_debug: overrides.is_debug ?? false,
    sha256: overrides.sha256 ?? 'fake_hash',
    ...overrides,
  };
}

export function group(
  componentId: string,
  technology: string,
  current: string | null,
  candidates: Candidate[],
): GameCandidateGroup {
  return {
    component_id: componentId,
    technology,
    file_path: `C:/Game/${componentId}.dll`,
    current_version: current,
    candidates,
  };
}

export function details(
  components: GameGraphicsComponent[],
  candidate_groups: GameCandidateGroup[],
): GameDetails {
  return { components, candidate_groups } as unknown as GameDetails;
}
