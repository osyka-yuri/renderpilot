import type { GameCardsQuery, GameCardsSortDirection, GameCardsSortField } from './types';
import { normalizeUniqueTrimmedStrings, trimToEmpty } from '@shared/utils/normalize';

const SORT_FIELDS = ['title', 'updates', 'risk'] as const satisfies readonly GameCardsSortField[];
const SORT_DIRECTIONS = ['asc', 'desc'] as const satisfies readonly GameCardsSortDirection[];

const SORT_FIELD_SET: ReadonlySet<string> = new Set(SORT_FIELDS);
const SORT_DIRECTION_SET: ReadonlySet<string> = new Set(SORT_DIRECTIONS);

const MIN_PAGE_LIMIT = 1;
const MIN_PAGE_OFFSET = 0;

function isReadonlyUnknownArray(value: unknown): value is readonly unknown[] {
  return Array.isArray(value);
}

function requirePlainObject(value: unknown, fieldName: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new TypeError(`${fieldName} must be an object.`);
  }

  return value as Record<string, unknown>;
}

function requireString(value: unknown, fieldName: string): string {
  if (typeof value !== 'string') {
    throw new TypeError(`${fieldName} must be a string.`);
  }

  return value;
}

function requireFiniteInteger(value: unknown, fieldName: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    throw new TypeError(`${fieldName} must be a finite number.`);
  }

  return Math.trunc(value);
}

function requireStringList(value: unknown, fieldName: string): string[] {
  if (!isReadonlyUnknownArray(value)) {
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
