import { describe, expect, it } from 'vitest';
import { presentDetectedEngines } from './file-safety-presenters';

describe('file safety presenters', () => {
  it('normalizes, deduplicates, and filters engine names for the confirmation list', () => {
    expect(presentDetectedEngines([' EasyAntiCheat ', 'BattlEye', 'battleye', '  '])).toEqual([
      'Easy Anti-Cheat',
      'BattlEye',
    ]);
  });
});
