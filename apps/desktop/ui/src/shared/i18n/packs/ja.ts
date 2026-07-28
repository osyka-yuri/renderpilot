import { ja } from '../messages/ja';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/ja';
import type { LocalePack } from './types';

const jaPack = {
  locale: 'ja',
  messages: ja,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default jaPack;
