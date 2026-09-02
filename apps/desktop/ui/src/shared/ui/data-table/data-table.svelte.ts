import {
  batch,
  createAtom,
  useSelector,
  type Atom,
  type AtomOptions,
} from '@tanstack/svelte-store';
import {
  constructTable,
  type RowData,
  type Table,
  type TableFeatures,
  type TableOptions,
} from '@tanstack/table-core';
import type { TableReactivityBindings } from '@tanstack/table-core/reactivity';
import { untrack } from 'svelte';

import { track } from '@shared/reactivity';

type SvelteTableOptions<TFeatures extends TableFeatures, TData extends RowData> = Omit<
  TableOptions<TFeatures, TData>,
  'features'
>;

/**
 * Creates a stable v9 TanStack table object for Svelte rune components.
 */
export function createSvelteTable<TFeatures extends TableFeatures, TData extends RowData>(
  features: TFeatures,
  options: SvelteTableOptions<TFeatures, TData>,
): Table<TFeatures, TData> {
  const svelteFeatures = {
    ...features,
    coreReactivityFeature: createSvelteReactivityBindings(),
  } as TFeatures;
  const tableOptions = createTableOptions(svelteFeatures, options);
  const table = constructTable(tableOptions);
  const optionsStore = table.optionsStore;
  if (optionsStore === undefined) {
    throw new Error('Svelte table construction requires an options store.');
  }

  const optionSnapshot = useSelector(optionsStore);
  const stateSnapshot = useSelector(table.store);
  let revision = $state(0);

  $effect(() => {
    track(optionSnapshot.current, stateSnapshot.current);
    revision = untrack(() => revision) + 1;
  });

  $effect.pre(() => {
    trackOptionValues(options);
    const nextOptions = createTableOptions(svelteFeatures, options);

    untrack(() => {
      table.setOptions(() => nextOptions);
    });
  });

  return new Proxy(table, {
    get(target, key): unknown {
      // Reading through the table facade subscribes the caller to both option
      // publication and feature-state writes through the official Svelte store
      // bridge, while the underlying table instance remains stable.
      track(revision);

      const value: unknown = Reflect.get(target, key, target);
      if (typeof value !== 'function') {
        return value;
      }

      const boundValue: (...args: unknown[]) => unknown = (...args) =>
        Reflect.apply(value, target, args) as unknown;
      return boundValue;
    },
  });
}

function createSvelteReactivityBindings(): TableReactivityBindings {
  return {
    createOptionsStore: true,
    wrapExternalAtoms: false,
    addSubscription: () => {
      throw new Error('Svelte table reactivity does not support external subscriptions.');
    },
    unmount: () => undefined,
    batch,
    schedule: queueMicrotask,
    untrack,
    createWritableAtom: <T>(initialValue: T, atomOptions?: AtomOptions<T>): Atom<T> =>
      createAtom(initialValue, atomOptions),
    createReadonlyAtom: <T>(resolve: () => T, atomOptions?: AtomOptions<T>) =>
      createAtom(resolve, atomOptions),
  };
}

function createTableOptions<TFeatures extends TableFeatures, TData extends RowData>(
  features: TFeatures,
  options: SvelteTableOptions<TFeatures, TData>,
): TableOptions<TFeatures, TData> {
  return mergeObjects({ features }, options) as TableOptions<TFeatures, TData>;
}

function trackOptionValues(options: object): void {
  for (const key of Reflect.ownKeys(options)) {
    const value = readProperty(options, key);

    if (key !== 'state' || typeof value !== 'object' || value === null) {
      continue;
    }

    for (const stateKey of Reflect.ownKeys(value)) {
      readProperty(value, stateKey);
    }
  }
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
