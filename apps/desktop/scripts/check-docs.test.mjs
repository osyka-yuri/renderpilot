import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { headingAnchors, localReferences, markdownWithoutFencedCode } from './check-docs.mjs';

describe('documentation Markdown parsing', () => {
  it('ignores Markdown and HTML references inside fenced code', () => {
    const markdown = `
[real link](guide.md#start)

\`\`\`markdown
[example link](missing.md)
![example image](missing.webp)
<a href="also-missing.md">example</a>
\`\`\`

~~~html
<img src="missing-again.webp" alt="Example">
~~~

![real image](screenshot.webp)
`;

    assert.deepEqual(localReferences(markdown), [
      { destination: 'guide.md#start', isImage: false, alt: 'real link' },
      { destination: 'screenshot.webp', isImage: true, alt: 'real image' },
    ]);
  });

  it('recognizes longer closing fences and preserves surrounding prose', () => {
    const markdown = `before
\`\`\`\`text
hidden
\`\`\`\`\`
after`;

    assert.equal(markdownWithoutFencedCode(markdown), 'before\n\n\n\nafter');
  });

  it('builds deterministic GitHub-style anchors outside fenced examples', () => {
    const markdown = `
## Safety and privacy
## Safety and privacy

\`\`\`
## Not a real heading
\`\`\`

## Français et 日本語
`;

    assert.deepEqual(
      [...headingAnchors(markdown)],
      ['safety-and-privacy', 'safety-and-privacy-1', 'français-et-日本語'],
    );
  });
});
