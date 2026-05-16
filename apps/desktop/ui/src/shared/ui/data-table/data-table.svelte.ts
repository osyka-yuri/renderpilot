import {
  type RowData,
  type TableOptions,
  type TableOptionsResolved,
  type TableState,
  type Updater,
  createTable,
} from '@tanstack/table-core';

/**
 * Creates a reactive TanStack table object for Svelte.
 *
 * Keeps TanStack's internal table state in Svelte state, while still allowing
 * externally controlled state via `options.state`.
 */
export function createSvelteTable<TData extends RowData>(options: TableOptions<TData>) {
  const defaultOptions = {
    state: {},
    onStateChange: noop,
    renderFallbackValue: null,
    mergeOptions: (
      defaultOptions: TableOptions<TData>,
      nextOptions: Partial<TableOptions<TData>>,
    ): TableOptions<TData> => {
      return mergeObjects(defaultOptions, nextOptions);
    },
  } satisfies Partial<TableOptionsResolved<TData>>;

  const resolvedOptions = mergeObjects(defaultOptions, options) as TableOptionsResolved<TData>;

  const table = createTable(resolvedOptions);
  let state = $state<TableState>(table.initialState);

  const tableState = mergeObjects(
    () => state,
    () => options.state ?? {},
  );

  const tableOptions = mergeObjects(resolvedOptions, {
    state: tableState,
    onStateChange: handleStateChange,
  }) as TableOptionsResolved<TData>;

  function handleStateChange(updater: Updater<TableState>): void {
    state = applyTableStateUpdater(updater, state);
    options.onStateChange?.(updater);
  }

  function syncOptions(): void {
    table.setOptions(() => tableOptions);
  }

  syncOptions();

  $effect.pre(() => {
    syncOptions();
  });

  return table;
}

function noop(): void {
  return undefined;
}

function applyTableStateUpdater(
  updater: Updater<TableState>,
  currentState: TableState,
): TableState {
  if (typeof updater === 'function') {
    return updater(currentState);
  }

  return {
    ...currentState,
    ...updater,
  };
}

type MaybeThunk<T extends object> = T | null | undefined | (() => T | null | undefined);

type ResolvedSource<T> = T extends () => infer Result ? NonNullable<Result> : NonNullable<T>;

type UnionToIntersection<T> = (T extends unknown ? (value: T) => void : never) extends (
  value: infer Result,
) => void
  ? Result
  : never;

type MergedSources<Sources extends readonly MaybeThunk<object>[]> = UnionToIntersection<
  ResolvedSource<Sources[number]>
> & {};

function isThunk<T extends object>(source: MaybeThunk<T>): source is () => T | null | undefined {
  return typeof source === 'function';
}

function resolveSource(source: MaybeThunk<object>): object | undefined {
  if (isThunk(source)) {
    return source() ?? undefined;
  }

  return source ?? undefined;
}

function readProperty(source: object, key: PropertyKey): unknown {
  return (source as Partial<Record<PropertyKey, unknown>>)[key];
}

function findOwnPropertyDescriptor(
  source: object,
  key: PropertyKey,
): PropertyDescriptor | undefined {
  return Object.getOwnPropertyDescriptor(source, key);
}

function pushUniqueKey(keys: (string | symbol)[], key: string | symbol): void {
  if (!keys.includes(key)) {
    keys.push(key);
  }
}

/**
 * Lazily merges multiple object-like sources by property key.
 *
 * Features:
 * - later sources override earlier sources;
 * - sources may be plain objects or lazy thunks;
 * - getters stay lazy and are resolved only when a property is read;
 * - property lookup supports inherited properties via `key in source`;
 * - own key enumeration stays deterministic;
 * - returned proxy is intentionally read-only.
 */
export function mergeObjects<Sources extends readonly MaybeThunk<object>[]>(
  ...sources: Sources
): MergedSources<Sources> {
  function findSourceWithKey(key: PropertyKey): object | undefined {
    for (let index = sources.length - 1; index >= 0; index -= 1) {
      const source = resolveSource(sources[index]);

      if (source && key in source) {
        return source;
      }
    }

    return undefined;
  }

  const handler: ProxyHandler<object> = {
    get(_, key): unknown {
      const source = findSourceWithKey(key);

      if (!source) {
        return undefined;
      }

      return readProperty(source, key);
    },

    has(_, key): boolean {
      return findSourceWithKey(key) !== undefined;
    },

    ownKeys(): (string | symbol)[] {
      const keys: (string | symbol)[] = [];

      for (const maybeSource of sources) {
        const source = resolveSource(maybeSource);

        if (!source) {
          continue;
        }

        for (const key of Object.getOwnPropertyNames(source)) {
          pushUniqueKey(keys, key);
        }

        for (const key of Object.getOwnPropertySymbols(source)) {
          pushUniqueKey(keys, key);
        }
      }

      return keys;
    },

    getOwnPropertyDescriptor(_, key): PropertyDescriptor | undefined {
      const source = findSourceWithKey(key);

      if (!source) {
        return undefined;
      }

      const descriptor = findOwnPropertyDescriptor(source, key);

      if (!descriptor) {
        return undefined;
      }

      return {
        configurable: true,
        enumerable: descriptor.enumerable,
        get: (): unknown => readProperty(source, key),
      };
    },

    set(): boolean {
      return false;
    },

    defineProperty(): boolean {
      return false;
    },

    deleteProperty(): boolean {
      return false;
    },
  };

  return new Proxy({}, handler) as MergedSources<Sources>;
}
