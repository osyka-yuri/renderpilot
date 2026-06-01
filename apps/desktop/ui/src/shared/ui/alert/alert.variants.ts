import { tv } from 'tailwind-variants';

export const alertVariants = tv({
  base: 'relative grid w-full grid-cols-[0_1fr] items-start gap-y-0.5 border [&>svg]:translate-y-0.5 [&>svg]:text-current',
  variants: {
    variant: {
      default: 'bg-card text-card-foreground',
      destructive:
        'text-destructive bg-card *:data-[slot=alert-description]:text-destructive/90 *:[svg]:text-current',
      warning:
        'border-warning/40 bg-warning/10 text-warning *:data-[slot=alert-description]:text-warning/90',
    },
    size: {
      default:
        'rounded-lg px-4 py-3 text-sm has-[>svg]:grid-cols-[--spacing(4)_1fr] has-[>svg]:gap-x-3 [&>svg]:size-4',
      sm: 'rounded-md px-3 py-2 text-xs has-[>svg]:grid-cols-[--spacing(3.5)_1fr] has-[>svg]:gap-x-2 [&>svg]:size-3.5',
    },
  },
  defaultVariants: {
    variant: 'default',
    size: 'default',
  },
});
