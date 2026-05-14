import type { GameCardsQuery, GameCardsSortDirection, GameCardsSortField } from '../model/types';
import { isArray, isNumber, isRecord, isString } from '@shared/validation';
import { normalizeUniqueTrimmedStrings, trimToEmpty } from '@shared/text';

const SORT_FIELDS = ['title', 'updates', 'risk'] as const satisfies readonly GameCardsSortField[];
const SORT_DIRECTIONS = ['asc', 'desc'] as const satisfies readonly GameCardsSortDirection[];

const SORT_FIELD_SET: ReadonlySet<string> = new Set(SORT_FIELDS);
const SORT_DIRECTION_SET: ReadonlySet<string> = new Set(SORT_DIRECTIONS);

export const DEFAULT_DESKTOP_GAME_CARDS_PAGE_LIMIT = 10_000;

export const DEFAULT_GAME_CARDS_CATALOG_SORT = {
  field: 'title',
  direction: 'asc',
} as const satisfies GameCardsQuery['sort'];

export const DEFAULT_GAME_CARDS_CATALOG_PAGE = {
  limit: DEFAULT_DESKTOP_GAME_CARDS_PAGE_LIMIT,
  offset: 0,
} as const satisfies GameCardsQuery['page'];

const MIN_PAGE_LIMIT = 1;
const MIN_PAGE_OFFSET = 0;

function requirePlainObject(value: unknown, fieldName: string): Record<string, unknown> {
  if (!isRecord(value) || isArray(value)) {
    throw new TypeError(`${fieldName} must be an object.`);
  }

  return value;
}

function requireString(value: unknown, fieldName: string): string {
  if (!isString(value)) {
    throw new TypeError(`${fieldName} must be a string.`);
  }

  return value;
}

function requireFiniteInteger(value: unknown, fieldName: string): number {
  if (!isNumber(value) || !Number.isFinite(value)) {
    throw new TypeError(`${fieldName} must be a finite number.`);
  }

  return Math.trunc(value);
}

function requireStringList(value: unknown, fieldName: string): string[] {
  if (!isArray(value)) {
    throw new TypeError(`${fieldName} must be an array.`);
  }

  return value.map((item, index) => requireString(item, `${fieldName}[${index}]`));
}

function requireSortField(value: unknown): GameCardsSortField {
  const sortField = requireString(value, 'query.sort.field');

  if (!SORT_FIELD_SET.has(sortField)) {
    throw new TypeError(`query.sort.field must be one of: ${SORT_FIELDS.join(', ')}.`);
  }

  return sortField as GameCardsSortField;
}

function requireSortDirection(value: unknown): GameCardsSortDirection {
  const sortDirection = requireString(value, 'query.sort.direction');

  if (!SORT_DIRECTION_SET.has(sortDirection)) {
    throw new TypeError(`query.sort.direction must be one of: ${SORT_DIRECTIONS.join(', ')}.`);
  }

  return sortDirection as GameCardsSortDirection;
}

function normalizeStringList(value: unknown, fieldName: string): string[] {
  return normalizeUniqueTrimmedStrings(requireStringList(value, fieldName));
}

function normalizePageLimit(value: unknown): number {
  return Math.max(MIN_PAGE_LIMIT, requireFiniteInteger(value, 'query.page.limit'));
}

function normalizePageOffset(value: unknown): number {
  return Math.max(MIN_PAGE_OFFSET, requireFiniteInteger(value, 'query.page.offset'));
}

export function normalizeGameCardsQuery(value: unknown): GameCardsQuery {
  const query = requirePlainObject(value, 'query');
  const sort = requirePlainObject(query.sort, 'query.sort');
  const page = requirePlainObject(query.page, 'query.page');

  return {
    searchQuery: trimToEmpty(requireString(query.searchQuery, 'query.searchQuery')),
    selectedLibraries: normalizeStringList(query.selectedLibraries, 'query.selectedLibraries'),
    selectedLaunchers: normalizeStringList(query.selectedLaunchers, 'query.selectedLaunchers'),
    sort: {
      field: requireSortField(sort.field),
      direction: requireSortDirection(sort.direction),
    },
    page: {
      limit: normalizePageLimit(page.limit),
      offset: normalizePageOffset(page.offset),
    },
  };
}
