import { zh } from '../messages/zh';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/zh';
import type { LocalePack } from './types';

const zhPack = {
  locale: 'zh',
  messages: zh,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default zhPack;
