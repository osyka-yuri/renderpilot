import type { BadgeSurface, BadgeTone } from './badge-types';

export type AccordionBadge = {
  label: string;
  tone?: BadgeTone;
  surface?: BadgeSurface;
};

export type AccordionItem = {
  value: string;
  title: string;
  summary?: string;
  meta?: string;
  badges?: AccordionBadge[];
  disabled?: boolean;
};
