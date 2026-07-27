import type { GamesCatalogScrollAnchor } from '@entities/game';

import type { GameCardState, LauncherGroup } from './launcher-groups';

export type GamesVirtualRow =
  | { kind: 'header'; key: string; label: string }
  | {
      kind: 'cards';
      key: string;
      cards: readonly GameCardState[];
    };

export type IndexedVirtualRow = {
  index: number;
};

export type MeasuredVirtualRow = IndexedVirtualRow & {
  start: number;
  end: number;
};

export type RenderedGamesVirtualRow<TVirtualRow extends IndexedVirtualRow> = {
  virtualRow: TVirtualRow;
  row: GamesVirtualRow;
};

/** Flattens launcher groups into stable header/card rows for virtualization. */
export function buildGamesVirtualRows(
  groups: readonly LauncherGroup[],
  columnCount: number,
): GamesVirtualRow[] {
  const safeColumnCount = Math.max(1, Math.trunc(columnCount));
  const rows: GamesVirtualRow[] = [];

  for (const group of groups) {
    rows.push({ kind: 'header', key: `header:${group.launcher}`, label: group.label });
    for (let index = 0; index < group.cards.length; index += safeColumnCount) {
      const cards = group.cards.slice(index, index + safeColumnCount);
      rows.push({
        kind: 'cards',
        key: `cards:${group.launcher}:${cards[0]?.id ?? index}`,
        cards,
      });
    }
  }

  return rows;
}

/**
 * Resolves virtualizer measurements against the current immutable row set.
 *
 * A virtualizer may retain measurements from the previous item count for one
 * reactive turn. Stale measurements are intentionally omitted so consumers
 * never dereference a row that was removed by an atomic filter/page update.
 */
export function pairExistingVirtualRows<TVirtualRow extends IndexedVirtualRow>(
  virtualRows: readonly TVirtualRow[],
  rows: readonly GamesVirtualRow[],
): RenderedGamesVirtualRow<TVirtualRow>[] {
  return virtualRows.flatMap((virtualRow) => {
    const row = gameVirtualRowAt(rows, virtualRow.index);
    return row ? [{ virtualRow, row }] : [];
  });
}

/** Returns one current row only when the virtualizer index is in bounds. */
export function gameVirtualRowAt(
  rows: readonly GamesVirtualRow[],
  index: number,
): GamesVirtualRow | undefined {
  if (!Number.isInteger(index) || index < 0 || index >= rows.length) {
    return undefined;
  }
  return rows[index];
}

export function gamesGridColumnCount(width: number, cardWidth = 328, gap = 12): number {
  return Math.max(1, Math.floor((Math.max(0, width) + gap) / (cardWidth + gap)));
}

export function shouldLoadMoreRows(
  rowCount: number,
  lastVisibleRowIndex: number,
  threshold = 3,
): boolean {
  return rowCount > 0 && lastVisibleRowIndex >= rowCount - Math.max(1, threshold);
}

export function findVisibleGamesAnchor(
  rows: readonly GamesVirtualRow[],
  measurements: readonly MeasuredVirtualRow[],
  scrollTop: number,
): GamesCatalogScrollAnchor | null {
  const measurement = measurements.find((candidate) => {
    const row = gameVirtualRowAt(rows, candidate.index);
    return (
      row !== undefined && candidate.end > scrollTop && row.kind === 'cards' && row.cards.length > 0
    );
  });
  if (!measurement) {
    return null;
  }
  const row = gameVirtualRowAt(rows, measurement.index);
  if (row?.kind !== 'cards' || row.cards.length === 0) {
    return null;
  }
  return {
    gameId: row.cards[0].id,
    offsetWithinRow: scrollTop - measurement.start,
  };
}

export function findGameVirtualRowIndex(rows: readonly GamesVirtualRow[], gameId: string): number {
  return rows.findIndex(
    (row) => row.kind === 'cards' && row.cards.some((card) => card.id === gameId),
  );
}
