import { describe, expect, it } from 'vitest';

import { CatalogDeltaAccumulator } from './catalog-delta-accumulator';

describe('CatalogDeltaAccumulator', () => {
  it('rejects stale revisions and normalizes ids with removed taking precedence', () => {
    const accumulator = new CatalogDeltaAccumulator();

    expect(
      accumulator.accept(
        {
          revision: 2,
          reasons: ['scan', 'live_facts'],
          changedGameIds: [' game ', 'removed', 'game'],
          removedGameIds: ['removed'],
        },
        1,
      ),
    ).toBe(true);
    expect(
      accumulator.accept({ revision: 2, reasons: [], changedGameIds: [], removedGameIds: [] }, 1),
    ).toBe(false);
    expect(accumulator.pending(1)).toEqual({
      revision: 2,
      reasons: ['scan', 'live_facts'],
      changedGameIds: ['game'],
      removedGameIds: ['removed'],
    });
  });

  it('clears pending facts only after the matching revision is installed', () => {
    const accumulator = new CatalogDeltaAccumulator();
    accumulator.accept(
      { revision: 4, reasons: ['scan'], changedGameIds: ['game'], removedGameIds: [] },
      1,
    );

    accumulator.reconcile(3);
    expect(accumulator.pending(3)?.revision).toBe(4);

    accumulator.reconcile(4);
    expect(accumulator.pending(4)).toBeNull();
  });

  it('keeps a removed id dominant while newer deltas are accumulated', () => {
    const accumulator = new CatalogDeltaAccumulator();
    accumulator.accept(
      { revision: 2, reasons: ['scan'], changedGameIds: [], removedGameIds: ['game'] },
      1,
    );
    accumulator.accept(
      { revision: 3, reasons: ['live_facts'], changedGameIds: ['game'], removedGameIds: [] },
      1,
    );

    expect(accumulator.pending(1)).toMatchObject({
      revision: 3,
      changedGameIds: [],
      removedGameIds: ['game'],
    });
  });
});
