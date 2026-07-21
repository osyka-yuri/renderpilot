import { describe, expect, it } from 'vitest';

import { fileNameFromPath, parentPath, sharedParentPath } from './path';

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

  describe('parentPath', () => {
    it.each([
      ['C:\\games\\a.dll', 'C:\\games'],
      ['C:/games/a.dll', 'C:/games'],
      ['/usr/lib/a.so', '/usr/lib'],
      ['C:/a.dll', 'C:/'],
      ['C:\\a.dll', 'C:\\'],
      ['/a.so', '/'],
      ['a.dll', null],
    ])('returns the parent of %s', (path, expected) => {
      expect(parentPath(path)).toBe(expected);
    });
  });

  describe('sharedParentPath', () => {
    it('matches Windows parents across separator and case differences', () => {
      expect(sharedParentPath(['C:/Games/DXC/dxcompiler.dll', 'c:\\games\\dxc\\dxil.dll'])).toBe(
        'C:/Games/DXC',
      );
    });

    it('preserves case sensitivity for Unix paths', () => {
      expect(sharedParentPath(['/opt/Game/a.so', '/opt/game/b.so'])).toBeNull();
    });

    it('returns null for different directories, bare names, and empty input', () => {
      expect(sharedParentPath(['C:/one/a.dll', 'C:/two/b.dll'])).toBeNull();
      expect(sharedParentPath(['a.dll', 'b.dll'])).toBeNull();
      expect(sharedParentPath([])).toBeNull();
    });
  });
});
