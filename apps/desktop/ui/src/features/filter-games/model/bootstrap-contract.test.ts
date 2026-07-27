import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  ALL_ADDON_CAPABILITIES,
  ALL_KNOWN_LAUNCHERS,
  expandLibraryFilterAliases,
} from '@entities/game';
import { ALL_KNOWN_LIBRARIES } from '@shared/graphics';
import { resolveGamesFiltersBootstrap } from './bootstrap-contract';

type ContractCase = {
  name: string;
  persisted: Record<string, unknown>;
  expected: Record<string, unknown>;
};

const contractCases = JSON.parse(
  readFileSync(
    new URL('../../../../../../../testdata/games-filter-bootstrap-cases.json', import.meta.url),
    'utf8',
  ),
) as ContractCase[];

describe('resolveGamesFiltersBootstrap', () => {
  it('normalizes a missing setting to the initial active technology filters', () => {
    const contract = resolveGamesFiltersBootstrap(null);

    expect(contract.selectedLibraries).toEqual(expandLibraryFilterAliases(ALL_KNOWN_LIBRARIES));
    expect(contract.selectedAddons).toEqual(ALL_ADDON_CAPABILITIES);
    expect(contract.selectedLaunchers).toEqual(ALL_KNOWN_LAUNCHERS);
    expect(contract.launcherOrder).toEqual(ALL_KNOWN_LAUNCHERS);
  });

  it.each(contractCases)('$name', ({ persisted, expected }) => {
    const contract = resolveGamesFiltersBootstrap(JSON.stringify(persisted));

    expect(contract.filters).toEqual(expected);
  });
});
