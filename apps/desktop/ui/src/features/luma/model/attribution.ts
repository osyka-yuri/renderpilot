import type { MessageKeyWithoutParams } from '@shared/i18n';

/** Upstream project credit shown on every Luma card surface. */
export const LUMA_ATTRIBUTION = {
  textKey: 'gameDetails.luma.attribution',
  linkKey: 'gameDetails.luma.attributionLink',
  href: 'https://github.com/Filoppi/Luma-Framework',
} as const satisfies {
  textKey: MessageKeyWithoutParams;
  linkKey: MessageKeyWithoutParams;
  href: string;
};
