import type { WithElementRef } from '../types';
import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';
import type { VariantProps } from 'tailwind-variants';
import type { buttonVariants } from './button.variants';

export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
  WithElementRef<HTMLAnchorAttributes> & {
    variant?: ButtonVariant;
    size?: ButtonSize;
  };
