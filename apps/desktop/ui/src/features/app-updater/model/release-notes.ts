/**
 * Bounded release-notes parser for the project changelog subset.
 *
 * Supports ## / ### headings, unordered lists, paragraphs, **strong**,
 * `code`, and Markdown links flattened to their visible label.
 *
 * Never produces HTML. All values are plain text safe for Svelte text nodes.
 */

export type ReleaseNotesInline =
  | {
      type: 'text';
      value: string;
    }
  | {
      type: 'strong';
      value: string;
    }
  | {
      type: 'code';
      value: string;
    };

export type ReleaseNotesBlock =
  | {
      type: 'heading';
      level: 2 | 3;
      content: ReleaseNotesInline[];
    }
  | {
      type: 'paragraph';
      content: ReleaseNotesInline[];
    }
  | {
      type: 'list';
      items: ReleaseNotesInline[][];
    };

export type ReleaseNotesDocument = {
  blocks: ReleaseNotesBlock[];
  truncated: boolean;
};

export const MAX_RELEASE_NOTES_CHARS = 50_000;
export const MAX_RELEASE_NOTES_BLOCKS = 250;
export const MAX_LIST_ITEMS = 500;
export const MAX_INLINE_SEGMENTS_PER_BLOCK = 200;

const EMPTY_DOCUMENT: ReleaseNotesDocument = { blocks: [], truncated: false };

export function parseReleaseNotes(input: string): ReleaseNotesDocument {
  if (typeof input !== 'string') {
    return EMPTY_DOCUMENT;
  }

  let truncated = false;
  let text = input.replace(/\r\n/g, '\n').replace(/\r/g, '\n').trim();

  if (text.length === 0) {
    return EMPTY_DOCUMENT;
  }

  if (text.length > MAX_RELEASE_NOTES_CHARS) {
    text = text.slice(0, MAX_RELEASE_NOTES_CHARS);
    truncated = true;
  }

  const lines = text.split('\n');
  const blocks: ReleaseNotesBlock[] = [];
  let listItemCount = 0;

  let i = 0;
  while (i < lines.length) {
    if (blocks.length >= MAX_RELEASE_NOTES_BLOCKS) {
      truncated = true;
      break;
    }

    const line = lines[i] ?? '';
    const trimmed = line.trim();

    if (trimmed.length === 0) {
      i += 1;
      continue;
    }

    const heading = matchHeading(trimmed);
    if (heading) {
      const { content, truncated: inlineTruncated } = parseInline(heading.text);
      if (inlineTruncated) {
        truncated = true;
      }
      blocks.push({ type: 'heading', level: heading.level, content });
      i += 1;
      continue;
    }

    if (isListItem(trimmed)) {
      const items: ReleaseNotesInline[][] = [];

      while (i < lines.length) {
        const listLine = (lines[i] ?? '').trim();
        if (listLine.length === 0) {
          break;
        }
        if (!isListItem(listLine)) {
          break;
        }
        if (blocks.length >= MAX_RELEASE_NOTES_BLOCKS) {
          truncated = true;
          break;
        }
        if (listItemCount >= MAX_LIST_ITEMS) {
          truncated = true;
          // Consume remaining consecutive list items without adding them.
          i += 1;
          while (i < lines.length && isListItem((lines[i] ?? '').trim())) {
            i += 1;
          }
          break;
        }

        const itemText = stripListMarker(listLine);
        const { content, truncated: inlineTruncated } = parseInline(itemText);
        if (inlineTruncated) {
          truncated = true;
        }
        items.push(content);
        listItemCount += 1;
        i += 1;
      }

      if (items.length > 0) {
        blocks.push({ type: 'list', items });
      }
      continue;
    }

    // Paragraph: consecutive non-empty, non-list, non-heading lines.
    const paragraphParts: string[] = [];
    while (i < lines.length) {
      const paraLine = (lines[i] ?? '').trim();
      if (paraLine.length === 0) {
        break;
      }
      if (matchHeading(paraLine) || isListItem(paraLine)) {
        break;
      }
      paragraphParts.push(paraLine);
      i += 1;
    }

    if (paragraphParts.length > 0) {
      const { content, truncated: inlineTruncated } = parseInline(paragraphParts.join(' '));
      if (inlineTruncated) {
        truncated = true;
      }
      blocks.push({ type: 'paragraph', content });
    }
  }

  return { blocks, truncated };
}

function matchHeading(line: string): { level: 2 | 3; text: string } | null {
  if (line.startsWith('### ')) {
    return { level: 3, text: line.slice(4) };
  }
  if (line.startsWith('## ')) {
    return { level: 2, text: line.slice(3) };
  }
  return null;
}

function isListItem(line: string): boolean {
  return line.startsWith('- ') || line.startsWith('* ');
}

function stripListMarker(line: string): string {
  return line.slice(2);
}

/**
 * Parse inline markdown: **strong**, `code`, [label](url) → label text.
 * Malformed markers stay as ordinary text. HTML is never interpreted.
 */
export function parseInline(text: string): {
  content: ReleaseNotesInline[];
  truncated: boolean;
} {
  const content: ReleaseNotesInline[] = [];
  let truncated = false;
  let cursor = 0;

  while (cursor < text.length) {
    if (content.length >= MAX_INLINE_SEGMENTS_PER_BLOCK) {
      truncated = true;
      break;
    }

    const remaining = text.slice(cursor);

    // Strong: **...**
    if (remaining.startsWith('**')) {
      const end = remaining.indexOf('**', 2);
      if (end > 2) {
        pushSegment(content, { type: 'strong', value: remaining.slice(2, end) });
        cursor += end + 2;
        continue;
      }
    }

    // Inline code: `...`
    if (remaining.startsWith('`')) {
      const end = remaining.indexOf('`', 1);
      if (end > 1) {
        pushSegment(content, { type: 'code', value: remaining.slice(1, end) });
        cursor += end + 1;
        continue;
      }
    }

    // Link: [label](url) → label as text
    if (remaining.startsWith('[')) {
      const labelEnd = remaining.indexOf(']');
      if (labelEnd > 1 && remaining[labelEnd + 1] === '(') {
        const urlEnd = remaining.indexOf(')', labelEnd + 2);
        if (urlEnd > labelEnd + 1) {
          pushSegment(content, { type: 'text', value: remaining.slice(1, labelEnd) });
          cursor += urlEnd + 1;
          continue;
        }
      }
    }

    // Ordinary text until the next special marker or end.
    const nextSpecial = findNextSpecialIndex(remaining);
    const end = nextSpecial === -1 ? remaining.length : Math.max(1, nextSpecial);
    // When nextSpecial is 0 but we failed to parse a marker, consume one char
    // so we never spin forever on a lone `*` or `[`.
    const sliceEnd = nextSpecial === 0 ? 1 : end;
    pushSegment(content, { type: 'text', value: remaining.slice(0, sliceEnd) });
    cursor += sliceEnd;
  }

  return { content: mergeAdjacentText(content), truncated };
}

function findNextSpecialIndex(text: string): number {
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (ch === '`' || ch === '[') {
      return i;
    }
    if (ch === '*' && text[i + 1] === '*') {
      return i;
    }
  }
  return -1;
}

function pushSegment(content: ReleaseNotesInline[], segment: ReleaseNotesInline): void {
  if (segment.value.length === 0) {
    return;
  }
  content.push(segment);
}

function mergeAdjacentText(segments: ReleaseNotesInline[]): ReleaseNotesInline[] {
  if (segments.length === 0) {
    return segments;
  }

  const merged: ReleaseNotesInline[] = [];
  for (const segment of segments) {
    const last = merged.length > 0 ? merged[merged.length - 1] : undefined;
    if (last && segment.type === 'text' && last.type === 'text') {
      last.value += segment.value;
    } else {
      merged.push({ ...segment });
    }
  }
  return merged;
}
