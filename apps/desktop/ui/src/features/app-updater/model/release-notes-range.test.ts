import { describe, expect, it } from 'vitest';

import { selectUpdateReleaseNotes } from './release-notes-range';

const HISTORY = [
  '# Changelog',
  '',
  '## [Unreleased]',
  '',
  '- Future work.',
  '',
  '## [1.9.5] - 2026-08-18',
  '',
  '- Fifth patch.',
  '',
  '## [1.9.4] - 2026-08-18',
  '',
  '- Fourth patch.',
  '',
  '## [1.9.3] - 2026-08-18',
  '',
  '- Third patch.',
  '',
  '## [1.9.2] - 2026-08-15',
  '',
  '- Second patch.',
  '',
  '## [1.9.1] - 2026-08-14',
  '',
  '- First patch.',
  '',
  '## [1.9.0] - 2026-08-12',
  '',
  '- Installed release.',
  '',
  '## [1.8.2] - 2026-07-30',
  '',
  '- Older release.',
].join('\n');

describe('selectUpdateReleaseNotes', () => {
  it('selects every intermediate release and excludes the installed release', () => {
    const notes = selectUpdateReleaseNotes(HISTORY, '1.9.0', '1.9.5');

    expect(notes).toContain('## [1.9.5]');
    expect(notes).toContain('## [1.9.4]');
    expect(notes).toContain('## [1.9.3]');
    expect(notes).toContain('## [1.9.2]');
    expect(notes).toContain('## [1.9.1]');
    expect(notes).not.toContain('## [1.9.0]');
    expect(notes).not.toContain('## [1.8.2]');
    expect(notes).not.toContain('Future work.');
  });

  it('keeps only the offered release for a one-version update', () => {
    const notes = selectUpdateReleaseNotes(HISTORY, '1.9.4', '1.9.5');

    expect(notes).toContain('## [1.9.5]');
    expect(notes).not.toContain('## [1.9.4]');
  });

  it('keeps all available history when the installed version predates the manifest history', () => {
    const notes = selectUpdateReleaseNotes(HISTORY, '1.0.0', '1.9.5');

    expect(notes).toContain('## [1.9.5]');
    expect(notes).toContain('## [1.8.2]');
    expect(notes).not.toContain('Future work.');
  });

  it('preserves legacy or custom notes when the offered heading is absent', () => {
    expect(selectUpdateReleaseNotes('Install the new release.', '1.9.0', '1.9.5')).toBe(
      'Install the new release.',
    );
    expect(selectUpdateReleaseNotes('## [1.9.4]\n\n- Different offer.', '1.9.0', '1.9.5')).toBe(
      '## [1.9.4]\n\n- Different offer.',
    );
  });

  it('does not interpret malformed legacy headings as release boundaries', () => {
    const notes = '## 1.9.5\n\n- Missing canonical brackets.';

    expect(selectUpdateReleaseNotes(notes, '1.9.0', '1.9.5')).toBe(notes);
  });

  it('normalizes line endings before returning the selected interval', () => {
    expect(
      selectUpdateReleaseNotes(
        '## [1.9.5]\r\n\r\n- New.\r\n\r\n## [1.9.4]\r\n\r\n- Installed.',
        '1.9.4',
        '1.9.5',
      ),
    ).toBe('## [1.9.5]\n\n- New.');
  });
});
