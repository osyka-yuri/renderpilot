import type { MessageKeyWithoutParams } from '@shared/i18n';

/** Upstream project credit shown on every RenoDX card surface. */
export const RENODX_ATTRIBUTION = {
  textKey: 'gameDetails.renodx.attribution',
  linkKey: 'gameDetails.renodx.attributionLink',
  href: 'https://github.com/clshortfuse/renodx',
} as const satisfies {
  textKey: MessageKeyWithoutParams;
  linkKey: MessageKeyWithoutParams;
  href: string;
};
