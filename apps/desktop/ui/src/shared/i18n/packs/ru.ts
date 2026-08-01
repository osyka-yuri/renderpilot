import { ru } from '../messages/ru';
import { MESSAGE_CONTRACT_VERSION } from '../messages/generated/contract-version';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/ru';
import { nvapiOverrides } from '../messages/overrides/nvapi/ru';
import type { LocalePack } from './types';

const ruPack = {
  locale: 'ru',
  contractVersion: MESSAGE_CONTRACT_VERSION,
  messages: ru,
  dynamicCatalogs: [lumaGuidanceOverrides, nvapiOverrides],
} as const satisfies LocalePack;

export default ruPack;
