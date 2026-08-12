import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  MAX_LIST_ITEMS,
  MAX_RELEASE_NOTES_BLOCKS,
  MAX_RELEASE_NOTES_CHARS,
  parseInline,
  parseReleaseNotes,
} from './release-notes';

const FIXTURE_DIR = import.meta.dirname;

describe('parseReleaseNotes', () => {
  it('returns empty for empty input', () => {
    expect(parseReleaseNotes('')).toEqual({ blocks: [], truncated: false });
  });

  it('returns empty for whitespace-only input', () => {
    expect(parseReleaseNotes('  \n\t  ')).toEqual({ blocks: [], truncated: false });
  });

  it('normalizes CRLF line endings', () => {
    const doc = parseReleaseNotes('## Title\r\n\r\n- item one\r\n- item two');
    expect(doc.blocks).toHaveLength(2);
    expect(doc.blocks[0]).toMatchObject({ type: 'heading', level: 2 });
    expect(doc.blocks[1]).toMatchObject({ type: 'list' });
  });

  it('parses heading only', () => {
    const doc = parseReleaseNotes('## Release');
    expect(doc.blocks).toEqual([
      {
        type: 'heading',
        level: 2,
        content: [{ type: 'text', value: 'Release' }],
      },
    ]);
  });

  it('parses list only and merges consecutive items', () => {
    const doc = parseReleaseNotes('- one\n* two\n- three');
    expect(doc.blocks).toHaveLength(1);
    expect(doc.blocks[0]).toMatchObject({ type: 'list' });
    if (doc.blocks[0]?.type === 'list') {
      expect(doc.blocks[0].items).toHaveLength(3);
    }
  });

  it('parses mixed headings, lists and paragraphs', () => {
    const doc = parseReleaseNotes(
      ['## Title', '', 'Intro line', 'continues here', '', '### Details', '', '- a', '- b'].join(
        '\n',
      ),
    );

    expect(doc.blocks.map((b) => b.type)).toEqual(['heading', 'paragraph', 'heading', 'list']);
  });

  it('joins multiline paragraphs with a single space', () => {
    const doc = parseReleaseNotes('Hello\nworld\nagain');
    expect(doc.blocks).toEqual([
      {
        type: 'paragraph',
        content: [{ type: 'text', value: 'Hello world again' }],
      },
    ]);
  });

  it('parses bold, code and flattens links', () => {
    const doc = parseReleaseNotes('See **important** and `cmd` and [label](https://example.com)');
    expect(doc.blocks[0]).toEqual({
      type: 'paragraph',
      content: [
        { type: 'text', value: 'See ' },
        { type: 'strong', value: 'important' },
        { type: 'text', value: ' and ' },
        { type: 'code', value: 'cmd' },
        { type: 'text', value: ' and label' },
      ],
    });
  });

  it('keeps malformed inline syntax as ordinary text', () => {
    const doc = parseReleaseNotes('**not closed and `also not');
    expect(doc.blocks[0]?.type).toBe('paragraph');
    if (doc.blocks[0]?.type === 'paragraph') {
      const text = doc.blocks[0].content.map((s) => s.value).join('');
      expect(text).toContain('**not closed');
      expect(text).toContain('`also not');
    }
  });

  it('keeps literal HTML tags as inert text', () => {
    const doc = parseReleaseNotes('<img src=x onerror=alert(1)>\n<script>alert(1)</script>');
    const text = JSON.stringify(doc);
    expect(text).toContain('<img src=x onerror=alert(1)>');
    expect(text).toContain('<script>alert(1)</script>');
    expect(doc.blocks.every((b) => b.type === 'paragraph')).toBe(true);
  });

  it('keeps generic syntax such as Array<T> and comparisons', () => {
    const doc = parseReleaseNotes('Use Array<T> when x < y');
    expect(doc.blocks[0]).toEqual({
      type: 'paragraph',
      content: [{ type: 'text', value: 'Use Array<T> when x < y' }],
    });
  });

  it('truncates by maximum character limit', () => {
    const input = 'a'.repeat(MAX_RELEASE_NOTES_CHARS + 100);
    const doc = parseReleaseNotes(input);
    expect(doc.truncated).toBe(true);
  });

  it('truncates by maximum block limit', () => {
    const lines = Array.from({ length: MAX_RELEASE_NOTES_BLOCKS + 10 }, (_, i) => `## H${i}`);
    const doc = parseReleaseNotes(lines.join('\n\n'));
    expect(doc.blocks.length).toBeLessThanOrEqual(MAX_RELEASE_NOTES_BLOCKS);
    expect(doc.truncated).toBe(true);
  });

  it('truncates by maximum list-item limit', () => {
    const lines = Array.from({ length: MAX_LIST_ITEMS + 20 }, (_, i) => `- item ${i}`);
    const doc = parseReleaseNotes(lines.join('\n'));
    expect(doc.truncated).toBe(true);
    if (doc.blocks[0]?.type === 'list') {
      expect(doc.blocks[0].items.length).toBeLessThanOrEqual(MAX_LIST_ITEMS);
    }
  });

  it('parses a real changelog fixture from the release pipeline', () => {
    const fixture = readFileSync(join(FIXTURE_DIR, 'fixtures/changelog-1.4.1.md'), 'utf8');
    const doc = parseReleaseNotes(fixture);

    expect(doc.truncated).toBe(false);
    expect(doc.blocks.some((b) => b.type === 'heading')).toBe(true);
    expect(doc.blocks.some((b) => b.type === 'list')).toBe(true);

    const serialized = JSON.stringify(doc);
    expect(serialized).toContain('RenoDX');
    expect(serialized).toContain('ReShade.ini');
  });
});

describe('parseInline', () => {
  it('parses strong segments', () => {
    expect(parseInline('**bold**').content).toEqual([{ type: 'strong', value: 'bold' }]);
  });

  it('parses inline code', () => {
    expect(parseInline('`code`').content).toEqual([{ type: 'code', value: 'code' }]);
  });

  it('flattens links to labels', () => {
    expect(parseInline('[docs](https://example.com)').content).toEqual([
      { type: 'text', value: 'docs' },
    ]);
  });
});
