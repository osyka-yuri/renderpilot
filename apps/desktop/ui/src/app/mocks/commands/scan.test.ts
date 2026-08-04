import { beforeEach, describe, expect, it } from 'vitest';

import { mockAddGame, mockInspectGameInstall, resetMockDesktopState } from '../desktop';

describe('preview manual scan commands', () => {
  beforeEach(() => {
    resetMockDesktopState();
  });

  it('uses the nearest catalog parent for a nested selection', async () => {
    const outer = await mockAddGame('D:/Games');
    const inner = await mockAddGame('D:/Games/Series');

    const inspection = await mockInspectGameInstall('D:\\Games\\Series\\Title');

    expect(inner.gameId).not.toBe(outer.gameId);
    expect(inspection.relationship).toEqual({
      kind: 'inside_existing',
      gameIds: [inner.gameId],
      provenInstallRoots: [],
    });
    expect(inspection.recommendation?.root).toBe('D:/Games/Series');
    expect(inspection.rootCorrection?.gameId).toBe(inner.gameId);
  });

  it('does not treat a common path prefix as containment', async () => {
    await mockAddGame('D:/Games/Title');

    const inspection = await mockInspectGameInstall('D:/Games/Title Deluxe/Binaries');

    expect(inspection.relationship).toEqual({
      kind: 'new',
      gameIds: [],
      provenInstallRoots: [],
    });
    expect(inspection.recommendation).toBeNull();
  });
});
