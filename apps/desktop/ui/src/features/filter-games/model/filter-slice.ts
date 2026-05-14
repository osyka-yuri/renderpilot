import { shallowStringArrayEqual } from '@shared/text';

export type FilterSlice = {
  applied: string[];
  draft: string[];
  deferSelectAll: boolean;
  pendingPersisted: string[] | null;
};

export type FilterSliceUpdate = Partial<FilterSlice>;

export function canonicalizeSelection(
  selected: readonly string[],
  available: readonly string[],
  normalizeFn: (values: readonly string[]) => string[],
  intersectFn: (selection: readonly string[], available: readonly string[]) => string[],
): string[] {
  const normalizedAvailable = normalizeFn(available);

  if (normalizedAvailable.length === 0) {
    return [];
  }

  const selectedSet = new Set(intersectFn(selected, normalizedAvailable));

  return normalizedAvailable.filter((value) => selectedSet.has(value));
}

export function createHydratedSlice(
  persistedValues: readonly string[] | null,
  availableValues: readonly string[],
  normalizeFn: (values: readonly string[]) => string[],
  canonicalizeFn: (selected: readonly string[], available: readonly string[]) => string[],
): FilterSlice {
  const normalizedPersisted = persistedValues !== null ? normalizeFn(persistedValues) : null;

  if (normalizedPersisted === null) {
    if (availableValues.length === 0) {
      return { applied: [], draft: [], deferSelectAll: true, pendingPersisted: null };
    }

    const selected = canonicalizeFn(availableValues, availableValues);

    return {
      applied: selected,
      draft: [...selected],
      deferSelectAll: false,
      pendingPersisted: null,
    };
  }

  if (availableValues.length === 0) {
    return {
      applied: [],
      draft: [],
      deferSelectAll: false,
      pendingPersisted: normalizedPersisted,
    };
  }

  const selected = canonicalizeFn(normalizedPersisted, availableValues);

  return { applied: selected, draft: [...selected], deferSelectAll: false, pendingPersisted: null };
}

export function createAvailableSliceUpdate(
  slice: FilterSlice,
  availableValues: readonly string[],
  canonicalizeFn: (selected: readonly string[], available: readonly string[]) => string[],
): FilterSliceUpdate | null {
  if (slice.deferSelectAll && availableValues.length > 0) {
    const selected = canonicalizeFn(availableValues, availableValues);

    return {
      applied: selected,
      draft: [...selected],
      deferSelectAll: false,
      pendingPersisted: null,
    };
  }

  if (slice.pendingPersisted !== null && availableValues.length > 0) {
    const selected = canonicalizeFn(slice.pendingPersisted, availableValues);

    return {
      applied: selected,
      draft: [...selected],
      deferSelectAll: false,
      pendingPersisted: null,
    };
  }

  if (availableValues.length === 0) {
    if (slice.applied.length === 0 && slice.draft.length === 0) {
      return null;
    }

    return {
      applied: [],
      draft: [],
      pendingPersisted: slice.pendingPersisted ? [...slice.pendingPersisted] : null,
    };
  }

  const applied = canonicalizeFn(slice.applied, availableValues);
  const draft = canonicalizeFn(slice.draft, availableValues);

  const hasAppliedChanged = !shallowStringArrayEqual(slice.applied, applied);
  const hasDraftChanged = !shallowStringArrayEqual(slice.draft, draft);

  if (!hasAppliedChanged && !hasDraftChanged) {
    return null;
  }

  const update: FilterSliceUpdate = {};

  if (hasAppliedChanged) {
    update.applied = applied;
  }

  if (hasDraftChanged) {
    update.draft = draft;
  }

  return update;
}

export function createSelectedSlice(values: readonly string[]): FilterSlice {
  const applied = [...values];

  return { applied, draft: [...applied], deferSelectAll: false, pendingPersisted: null };
}

export function createEmptySlice({
  deferSelectAll,
  pendingPersisted,
}: {
  deferSelectAll: boolean;
  pendingPersisted: readonly string[] | null;
}): FilterSlice {
  return {
    applied: [],
    draft: [],
    deferSelectAll,
    pendingPersisted: pendingPersisted ? [...pendingPersisted] : null,
  };
}

export function applySliceUpdate<T extends Record<string, unknown>>(
  state: T,
  update: Record<string, unknown> | null,
): T {
  if (update === null) {
    return state;
  }

  return { ...state, ...update };
}
