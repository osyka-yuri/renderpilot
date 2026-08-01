import { ja } from '../messages/ja';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/ja';
import type { LocalePack } from './types';

const jaPack = {
  locale: 'ja',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: ja,
  dynamicCatalogs: [lumaGuidanceOverrides],
} as const satisfies LocalePack;

export default jaPack;
