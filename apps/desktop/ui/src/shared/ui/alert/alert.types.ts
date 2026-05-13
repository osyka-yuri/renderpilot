import type { WithElementRef } from '../types';
import type { HTMLAttributes } from 'svelte/elements';
import type { VariantProps } from 'tailwind-variants';
import type { alertVariants } from './alert.variants';

export type AlertVariant = VariantProps<typeof alertVariants>['variant'];

export type AlertProps = WithElementRef<HTMLAttributes<HTMLDivElement>> & {
  variant?: AlertVariant;
};
