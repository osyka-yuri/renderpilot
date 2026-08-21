import { describe, expect, it } from 'vitest';
import {
  fileSafetyMessageKey,
  formatDetectedEngines,
  presentDetectedEngines,
  presentFileSafetyMessage,
} from './file-safety-presenters';

describe('file safety presenters', () => {
  it('uses the neutral message only when no engines are detected', () => {
    expect(fileSafetyMessageKey({ detected_engines: [] })).toBe('gameDetails.fileSafety.generic');
    expect(presentFileSafetyMessage({ detected_engines: [] })).toBe(
      'Changes to multiplayer game files may result in account restrictions or a ban.',
    );
    expect(
      fileSafetyMessageKey({ detected_engines: ['EasyAntiCheat'], scan_completeness: 'limited' }),
    ).toBe('gameDetails.fileSafety.detectedOne');
  });

  it('presents one canonical engine without adding a safety claim', () => {
    expect(presentDetectedEngines(['EasyAntiCheat'])).toEqual(['Easy Anti-Cheat']);
    const message = presentFileSafetyMessage({ detected_engines: ['EasyAntiCheat'] });
    expect(message).toContain('Easy Anti-Cheat');
    expect(message).toContain('account restrictions or a ban');
    const safeWord = ['s', 'a', 'f', 'e'].join('');
    expect(message.toLocaleLowerCase()).not.toContain(safeWord);
    expect(message.toLocaleLowerCase()).not.toContain(`${safeWord} likely ${safeWord}`);
  });

  it('deduplicates and presents multiple canonical engines', () => {
    expect(presentDetectedEngines(['EasyAntiCheat', 'BattlEye', 'battleye'])).toEqual([
      'Easy Anti-Cheat',
      'BattlEye',
    ]);
    expect(fileSafetyMessageKey({ detected_engines: ['EasyAntiCheat', 'BattlEye'] })).toBe(
      'gameDetails.fileSafety.detectedMany',
    );
    expect(formatDetectedEngines(['Easy Anti-Cheat', 'BattlEye'], 'ru')).toBe(
      'Easy Anti-Cheat и BattlEye',
    );
    expect(presentFileSafetyMessage({ detected_engines: ['EasyAntiCheat', 'BattlEye'] })).toContain(
      'Easy Anti-Cheat and BattlEye',
    );
  });
});
