import { en } from '../messages/en';
import type { LocalePack } from './types';

export const enPack = {
  locale: 'en',
  messages: en,
  dynamicCatalogs: [],
} as const satisfies LocalePack;

export default enPack;
