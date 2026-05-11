import { describe, expect, it } from 'vitest';
import { fileNameFromPath } from './path';

describe('path utils', () => {
  describe('fileNameFromPath', () => {
    it('extracts file name from Windows path', () => {
      expect(fileNameFromPath('C:\\games\\a.dll')).toBe('a.dll');
    });

    it('extracts file name from Unix path', () => {
      expect(fileNameFromPath('/usr/lib/b.so')).toBe('b.so');
    });

    it('returns original for path without separators', () => {
      expect(fileNameFromPath('readme.txt')).toBe('readme.txt');
    });
  });
});
