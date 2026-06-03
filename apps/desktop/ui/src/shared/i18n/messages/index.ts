import type { Locale } from '../locale';
import { en, type MessageKey } from './en';
import { ru } from './ru';
import { es } from './es';
import { zh } from './zh';
import { fr } from './fr';
import { de } from './de';
import { ja } from './ja';
import type { MessageDictionary } from './types';

export const messages: Record<Locale, MessageDictionary> = { en, ru, es, zh, fr, de, ja };

export { nvapiOverrides } from './nvapi';

export type { MessageKey };
