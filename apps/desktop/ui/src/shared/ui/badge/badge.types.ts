import type { WithElementRef } from '../types';
import type { HTMLAnchorAttributes } from 'svelte/elements';
import type { VariantProps } from 'tailwind-variants';
import type { badgeVariants } from './badge.variants';

export type BadgeVariant = VariantProps<typeof badgeVariants>['variant'];

export type BadgeProps = WithElementRef<HTMLAnchorAttributes> & {
  variant?: BadgeVariant;
};
