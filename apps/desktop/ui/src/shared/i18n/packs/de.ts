import { de } from '../messages/de';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/de';
import type { LocalePack } from './types';

const dePack = {
  locale: 'de',
  messages: de,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default dePack;
