<script
  lang="ts"
  generics="TData, TValue, TContext extends HeaderContext<TData, TValue> | CellContext<TData, TValue>"
>
  import type { CellContext, ColumnDefTemplate, HeaderContext } from '@tanstack/table-core';
  import type { Attachment } from 'svelte/attachments';

  import { RenderComponentConfig, RenderSnippetConfig } from './render-helpers.js';

  type Props = {
    /** The cell or header field of the current cell's column definition. */
    content?: TContext extends HeaderContext<TData, TValue>
      ? ColumnDefTemplate<HeaderContext<TData, TValue>>
      : TContext extends CellContext<TData, TValue>
        ? ColumnDefTemplate<CellContext<TData, TValue>>
        : never;

    /** The result of the `getContext()` function of the header or cell. */
    context: TContext;

    /** Used to pass attachments that can't be obtained through context. */
    attach?: Attachment;
  };

  type TemplateRenderer = (context: TContext) => unknown;
  type ComponentConfig = InstanceType<typeof RenderComponentConfig>;
  type SnippetConfig = InstanceType<typeof RenderSnippetConfig>;

  let { content, context, attach }: Props = $props();

  const rendered = $derived(resolveContent(content, context));

  function resolveContent(template: Props['content'], ctx: TContext): unknown {
    if (typeof template === 'string') return template;

    if (typeof template === 'function') {
      return (template as TemplateRenderer)(ctx);
    }

    return undefined;
  }

  function isComponentConfig(value: unknown): value is ComponentConfig {
    return value instanceof RenderComponentConfig;
  }

  function isSnippetConfig(value: unknown): value is SnippetConfig {
    return value instanceof RenderSnippetConfig;
  }

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
  }

  function toProps(value: unknown): Record<string, unknown> {
    return isRecord(value) ? value : {};
  }

  function withAttach(value: unknown): Record<string, unknown> {
    return { ...toProps(value), attach };
  }
</script>

{#if isComponentConfig(rendered)}
  {@const { component: Component, props } = rendered}
  <Component {...withAttach(props)} />
{:else if isSnippetConfig(rendered)}
  {@const { snippet, params } = rendered}
  {@render snippet(withAttach(params))}
{:else}
  {rendered}
{/if}
