import { ru } from '../messages/ru';
import { lumaGuidanceOverrides } from '../messages/overrides/luma/ru';
import { nvapiOverrides } from '../messages/overrides/nvapi/ru';
import type { LocalePack } from './types';

const ruPack = {
  locale: 'ru',
  messages: ru,
  dynamicCatalogs: [lumaGuidanceOverrides, nvapiOverrides],
} as const satisfies LocalePack;

export default ruPack;
