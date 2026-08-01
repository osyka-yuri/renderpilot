import type { Component, Snippet } from 'svelte';

type EmptyProps = Record<string, never>;

const EMPTY_PROPS: EmptyProps = Object.freeze({});

const RENDER_CONFIG_KIND: unique symbol = Symbol.for(
  '@tanstack/svelte-table/render-config',
) as never;

type RenderConfigKind = 'component' | 'snippet';

type RenderConfigBrand<TKind extends RenderConfigKind> = {
  readonly [RENDER_CONFIG_KIND]: TKind;
};

type RequiredKeys<T extends Record<string, unknown>> = {
  [K in keyof T]-?: EmptyProps extends Pick<T, K> ? never : K;
}[keyof T];

/*
 * Infer props separately from the component. Constraining the component type
 * directly defaults its props to {}, which rejects components with required props.
 */
type ComponentPropsArgument<TProps extends Record<string, unknown>> =
  RequiredKeys<TProps> extends never ? [props?: TProps] : [props: TProps];

type SnippetParamsArgument<TParams> = [TParams] extends [undefined]
  ? [params?: undefined]
  : [params: TParams];

function isRecord(value: unknown): value is Record<PropertyKey, unknown> {
  return typeof value === 'object' && value !== null;
}

/**
 * A helper class to make it easy to identify Svelte components in
 * `columnDef.cell` and `columnDef.header` properties.
 *
 * This class is intended for internal adapter usage.
 */
export class RenderComponentConfig<
  TProps extends Record<string, unknown> = Record<string, unknown>,
> implements RenderConfigBrand<'component'> {
  readonly [RENDER_CONFIG_KIND] = 'component';

  constructor(
    readonly component: Component<TProps>,
    readonly props: TProps | EmptyProps = EMPTY_PROPS,
  ) {}
}

/**
 * A helper class to make it easy to identify Svelte snippets in
 * `columnDef.cell` and `columnDef.header` properties.
 *
 * This class is intended for internal adapter usage.
 */
export class RenderSnippetConfig<TParams> implements RenderConfigBrand<'snippet'> {
  readonly [RENDER_CONFIG_KIND] = 'snippet';

  constructor(
    readonly snippet: Snippet<[TParams]>,
    readonly params: TParams,
  ) {}
}

/**
 * Checks whether a value is a component render config.
 *
 * Prefer this over `instanceof RenderComponentConfig`, because `instanceof`
 * can fail when the app contains multiple copies of this package.
 */
export function isRenderComponentConfig(value: unknown): value is RenderComponentConfig {
  return isRecord(value) && value[RENDER_CONFIG_KIND] === 'component';
}

/**
 * Checks whether a value is a snippet render config.
 *
 * Prefer this over `instanceof RenderSnippetConfig`, because `instanceof`
 * can fail when the app contains multiple copies of this package.
 */
export function isRenderSnippetConfig(value: unknown): value is RenderSnippetConfig<unknown> {
  return isRecord(value) && value[RENDER_CONFIG_KIND] === 'snippet';
}

/**
 * A helper function to create cells or headers from Svelte components through
 * ColumnDef's `cell` and `header` properties.
 *
 * This is only for Svelte components. Use `renderSnippet` for Svelte snippets.
 *
 * @example
 * ```ts
 * const defaultColumns = [
 *   columnHelper.accessor("name", {
 *     header: header => renderComponent(SortHeader, { label: "Name", header }),
 *   }),
 * ];
 * ```
 *
 * @see {@link https://tanstack.com/table/latest/docs/guide/column-defs}
 */
export function renderComponent<TProps extends Record<string, unknown>>(
  component: Component<TProps>,
  ...args: ComponentPropsArgument<TProps>
): RenderComponentConfig<TProps> {
  return new RenderComponentConfig(component, args[0] ?? EMPTY_PROPS);
}

/**
 * A helper function to create cells or headers from Svelte snippets through
 * ColumnDef's `cell` and `header` properties.
 *
 * The snippet must take exactly one parameter.
 *
 * This is only for Svelte snippets. Use `renderComponent` for Svelte components.
 *
 * @example
 * ```ts
 * const defaultColumns = [
 *   columnHelper.accessor("name", {
 *     cell: cell => renderSnippet(nameSnippet, {
 *       name: cell.row.original.name,
 *     }),
 *   }),
 * ];
 * ```
 *
 * @see {@link https://tanstack.com/table/latest/docs/guide/column-defs}
 */
export function renderSnippet<TParams>(
  snippet: Snippet<[TParams]>,
  ...args: SnippetParamsArgument<TParams>
): RenderSnippetConfig<TParams> {
  return new RenderSnippetConfig(snippet, args[0] as TParams);
}
