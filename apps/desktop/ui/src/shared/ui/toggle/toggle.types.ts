import type { VariantProps } from 'tailwind-variants';
import type { toggleVariants } from './toggle.variants';

export type ToggleVariant = VariantProps<typeof toggleVariants>['variant'];
export type ToggleSize = VariantProps<typeof toggleVariants>['size'];
export type ToggleVariants = VariantProps<typeof toggleVariants>;
