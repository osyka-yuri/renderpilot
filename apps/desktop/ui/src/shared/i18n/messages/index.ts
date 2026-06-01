import type { Locale } from '../locale';
import { en, type MessageKey } from './en';
import { ru } from './ru';
import type { MessageDictionary } from './types';

export const messages: Record<Locale, MessageDictionary> = { en, ru };

export type { MessageKey };
