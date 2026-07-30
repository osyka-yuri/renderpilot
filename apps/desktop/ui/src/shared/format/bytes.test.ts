import { describe, expect, it } from 'vitest';
import { formatBytes } from './bytes';

describe('formatBytes', () => {
  it('formats zero with the locale byte unit', () => {
    expect(formatBytes(0, 'en')).toBe('0 byte');
  });

  it('normalizes negative and non-finite values to zero', () => {
    expect(formatBytes(-1, 'en')).toBe('0 byte');
    expect(formatBytes(Number.NaN, 'en')).toBe('0 byte');
  });

  it('formats bytes', () => {
    expect(formatBytes(0.5, 'en')).toBe('0.5 byte');
    expect(formatBytes(512, 'en')).toBe('512 byte');
  });

  it('formats kilobytes', () => {
    expect(formatBytes(1024, 'en')).toBe('1 kB');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1_048_576, 'en')).toBe('1 MB');
  });

  it('formats gigabytes with one localized decimal', () => {
    expect(formatBytes(1_500_000_000, 'en')).toBe('1.4 GB');
    expect(formatBytes(1_500_000_000, 'ru')).toMatch(/^1,4\sГБ$/u);
    expect(formatBytes(1_500_000_000, 'fr')).toMatch(/^1,4\sGo$/u);
  });
});
