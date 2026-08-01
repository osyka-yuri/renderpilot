import type { MessageParams } from '@shared/i18n';

import { toolMessageKey } from './tool-message-key';

const lumaVersionKey = toolMessageKey('gameDetails.luma', 'host.version');
const renoDxVersionKey = toolMessageKey('gameDetails.renodx', 'host.version');
const lumaDateKey = toolMessageKey('gameDetails.luma', 'addonDated');
const renoDxDateKey = toolMessageKey('gameDetails.renodx', 'addonDated');

void lumaVersionKey;
void renoDxVersionKey;
void lumaDateKey;
void renoDxDateKey;

const lumaVersionParams: MessageParams<typeof lumaVersionKey> = { version: '1.2.3' };
const renoDxVersionParams: MessageParams<typeof renoDxVersionKey> = { version: 123 };
const lumaDateParams: MessageParams<typeof lumaDateKey> = { date: '2026-07-31' };
const renoDxDateParams: MessageParams<typeof renoDxDateKey> = { date: 1_775_088_000_000 };

void lumaVersionParams;
void renoDxVersionParams;
void lumaDateParams;
void renoDxDateParams;

// @ts-expect-error Arbitrary suffixes are outside the mirrored tool-key contract.
toolMessageKey('gameDetails.luma', 'arbitrary');
// @ts-expect-error Tool-specific suffixes are not part of the shared mirror.
toolMessageKey('gameDetails.renodx', 'launchArgs.title');
// @ts-expect-error Parameter names come from the selected catalog message.
const invalidVersionParams: MessageParams<typeof lumaVersionKey> = { date: '2026-07-31' };
void invalidVersionParams;
