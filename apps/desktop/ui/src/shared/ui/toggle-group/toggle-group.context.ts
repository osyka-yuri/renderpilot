import { getContext, setContext } from 'svelte';
import type { VariantProps } from 'tailwind-variants';

import type { toggleVariants } from '../toggle/toggle.variants';

type ToggleVariants = VariantProps<typeof toggleVariants>;

export type ToggleGroupVariant = NonNullable<ToggleVariants['variant']>;
export type ToggleGroupSize = NonNullable<ToggleVariants['size']>;

export type ToggleGroupContext = {
  readonly variant: ToggleGroupVariant;
  readonly size: ToggleGroupSize;
  readonly spacing: number;
};

const TOGGLE_GROUP_CONTEXT_KEY = Symbol('toggle-group');

export function setToggleGroupCtx(ctx: ToggleGroupContext): void {
  setContext(TOGGLE_GROUP_CONTEXT_KEY, ctx);
}

export function getToggleGroupCtx(): ToggleGroupContext {
  const ctx = getContext<ToggleGroupContext | undefined>(TOGGLE_GROUP_CONTEXT_KEY);

  if (!ctx) {
    throw new Error('ToggleGroupItem must be used inside ToggleGroup.');
  }

  return ctx;
}
