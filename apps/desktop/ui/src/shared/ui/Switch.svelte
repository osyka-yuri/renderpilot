<script lang="ts">
  type MouseHandler = (event: MouseEvent) => void;
  type FocusHandler = (event: FocusEvent) => void;

  export let checked = false;
  export let disabled = false;

  /**
   * Если не передать ariaLabel, accessible name будет браться из slot-контента.
   * Для сложного slot-контента лучше передавать ariaLabel явно.
   */
  export let ariaLabel: string | undefined = undefined;
  export let ariaLabelledBy: string | undefined = undefined;
  export let ariaDescribedBy: string | undefined = undefined;

  export let title: string | undefined = undefined;
  export let className = "";

  /**
   * Сохраняет старое поведение: на мобильных label и switch становятся колонкой.
   */
  export let stackOnMobile = true;

  export let onclick: MouseHandler | undefined = undefined;
  export let onfocus: FocusHandler | undefined = undefined;
  export let onblur: FocusHandler | undefined = undefined;

  $: state = checked ? "checked" : "unchecked";
  $: rootClass = ["switch-root", className].filter(Boolean).join(" ");

  $: normalizedAriaLabel = ariaLabel?.trim() || undefined;
  $: normalizedTitle = title?.trim() || undefined;

  function handleClick(event: MouseEvent) {
    if (disabled) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }

    onclick?.(event);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked ? "true" : "false"}
  aria-label={normalizedAriaLabel}
  aria-labelledby={ariaLabelledBy}
  aria-describedby={ariaDescribedBy}
  title={normalizedTitle}
  {disabled}
  class={rootClass}
  data-state={state}
  data-stack-on-mobile={stackOnMobile ? "true" : undefined}
  onclick={handleClick}
  {onfocus}
  {onblur}
>
  <span class="switch-copy">
    <slot />
  </span>

  <span class="switch-track" aria-hidden="true">
    <span class="switch-thumb"></span>
  </span>
</button>

<style>
  .switch-root,
  .switch-root * {
    box-sizing: border-box;
  }

  .switch-root {
    --switch-width: 2.5rem;
    --switch-height: 1.25rem;
    --switch-padding: 0.125rem;
    --switch-thumb-size: 0.875rem;

    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;

    border: 0;
    appearance: none;
    background: transparent;
    padding: 0;

    font: inherit;
    color: inherit;
    text-align: start;

    cursor: pointer;
    user-select: none;
    touch-action: manipulation;
  }

  .switch-copy {
    min-width: 0;
    flex: 1 1 auto;
  }

  .switch-track {
    position: relative;
    flex: 0 0 auto;

    inline-size: var(--switch-width);
    block-size: var(--switch-height);
    border: 1px solid var(--border-control);
    border-radius: 999px;

    background: var(--bg-control);

    transition:
      background-color 140ms ease,
      border-color 140ms ease,
      box-shadow 140ms ease;
  }

  .switch-thumb {
    position: absolute;
    inset-block-start: 50%;
    inset-inline-start: var(--switch-padding);

    inline-size: var(--switch-thumb-size);
    block-size: var(--switch-thumb-size);
    border-radius: 999px;

    background: var(--text-soft);
    transform: translateY(-50%);

    pointer-events: none;

    transition:
      inset-inline-start 140ms ease,
      background-color 140ms ease;
  }

  .switch-root[data-state="checked"] .switch-track {
    background: var(--accent);
    border-color: transparent;
  }

  .switch-root[data-state="checked"] .switch-thumb {
    inset-inline-start: calc(
      100% - var(--switch-padding) - var(--switch-thumb-size)
    );
    background: var(--accent-contrast);
  }

  .switch-root:focus-visible {
    outline: none;
  }

  .switch-root:focus-visible .switch-track {
    box-shadow: var(--shadow-focus);
  }

  @media (hover: hover) {
    .switch-root:not(:disabled):not([data-state="checked"]):hover
      .switch-track {
      background: var(--bg-control-hover);
    }

    .switch-root:not(:disabled)[data-state="checked"]:hover .switch-track {
      background: var(--accent-strong);
    }
  }

  .switch-root:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  @media (max-width: 720px) {
    .switch-root[data-stack-on-mobile="true"] {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .switch-root[data-stack-on-mobile="true"] .switch-track {
      align-self: flex-start;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .switch-track,
    .switch-thumb {
      transition: none;
    }
  }

  @media (forced-colors: active) {
    .switch-track {
      border-color: CanvasText;
    }

    .switch-root:focus-visible .switch-track {
      outline: 2px solid Highlight;
      outline-offset: 2px;
      box-shadow: none;
    }
  }
</style>
