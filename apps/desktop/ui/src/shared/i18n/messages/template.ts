export type MessageTemplateToken =
  | Readonly<{ kind: 'text'; value: string }>
  | Readonly<{ kind: 'placeholder'; name: string }>;

export type MessageTemplateAnalysis =
  | Readonly<{
      valid: true;
      placeholders: readonly string[];
      tokens: readonly MessageTemplateToken[];
    }>
  | Readonly<{ valid: false; placeholders: readonly []; tokens: readonly [] }>;

const INVALID_TEMPLATE: MessageTemplateAnalysis = {
  valid: false,
  placeholders: [],
  tokens: [],
};

function isPlaceholderCharacter(character: string): boolean {
  const code = character.charCodeAt(0);
  return (
    (code >= 48 && code <= 57) ||
    (code >= 65 && code <= 90) ||
    code === 95 ||
    (code >= 97 && code <= 122)
  );
}

/** Parses the authored `{name}` grammar shared by runtime and build-time tooling. */
export function analyzeMessageTemplate(template: string): MessageTemplateAnalysis {
  const tokens: MessageTemplateToken[] = [];
  const placeholders: string[] = [];
  const seenPlaceholders = new Set<string>();
  let textStart = 0;
  let cursor = 0;

  while (cursor < template.length) {
    const character = template[cursor];
    if (character === '}') {
      return INVALID_TEMPLATE;
    }
    if (character !== '{') {
      cursor += 1;
      continue;
    }

    if (textStart < cursor) {
      tokens.push({ kind: 'text', value: template.slice(textStart, cursor) });
    }

    const nameStart = cursor + 1;
    let nameEnd = nameStart;
    while (nameEnd < template.length && isPlaceholderCharacter(template[nameEnd])) {
      nameEnd += 1;
    }
    if (nameEnd === nameStart || template[nameEnd] !== '}') {
      return INVALID_TEMPLATE;
    }

    const name = template.slice(nameStart, nameEnd);
    tokens.push({ kind: 'placeholder', name });
    if (!seenPlaceholders.has(name)) {
      seenPlaceholders.add(name);
      placeholders.push(name);
    }

    cursor = nameEnd + 1;
    textStart = cursor;
  }

  if (textStart < template.length) {
    tokens.push({ kind: 'text', value: template.slice(textStart) });
  }

  return { valid: true, placeholders, tokens };
}
