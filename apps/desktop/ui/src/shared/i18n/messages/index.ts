import type { Locale } from '../locale';
import { en, type MessageKey } from './en';
import { ru } from './ru';
import { es } from './es';
import { zh } from './zh';
import { fr } from './fr';
import { de } from './de';
import { ja } from './ja';
import { lumaGuidanceOverrides } from './luma-guidance';
import { nvapiOverrides } from './nvapi';
import type { MessageDictionary } from './types';
import type { DynamicMessageCatalog } from '../lookup';

export const messages: Record<Locale, MessageDictionary> = { en, ru, es, zh, fr, de, ja };

/** Dynamic catalogs participate in one deterministic locale-first lookup. */
export const dynamicMessageCatalogs: readonly DynamicMessageCatalog[] = [
  lumaGuidanceOverrides,
  nvapiOverrides,
];

export { lumaGuidanceOverrides, nvapiOverrides };

export type { MessageKey };
