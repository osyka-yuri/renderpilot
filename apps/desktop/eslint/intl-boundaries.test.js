import path from 'node:path';

import { RuleTester } from 'eslint';
import { describe, it } from 'vitest';
import tseslint from 'typescript-eslint';

import { createIntlBoundariesRule } from './intl-boundaries.js';

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

const projectRoot = path.resolve('C:/project');
const sourceRoot = 'ui/src';
const featureFile = path.join(projectRoot, sourceRoot, 'features/example/model/value.ts');
const intlFile = path.join(projectRoot, sourceRoot, 'shared/intl/formatters.ts');
const formatFile = path.join(projectRoot, sourceRoot, 'shared/format/numbers.ts');
const i18nFile = path.join(projectRoot, sourceRoot, 'shared/i18n/format.ts');
const textFile = path.join(projectRoot, sourceRoot, 'shared/text/graphemes.ts');

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
});

ruleTester.run('intl-boundaries', createIntlBoundariesRule({ projectRoot, sourceRoot }), {
  valid: [
    {
      filename: intlFile,
      code: "const formatter = new Intl.NumberFormat('en');",
    },
    {
      filename: formatFile,
      code: "import { createNumberFormatter } from '@shared/intl';",
    },
    {
      filename: i18nFile,
      code: "import { createPluralRules } from '@/shared/intl/formatters';",
    },
    {
      filename: textFile,
      code: "import { createSegmenter } from '@shared/intl';",
    },
    {
      filename: featureFile,
      code: "import { formatPercent } from '@shared/format'; formatPercent(0.5, 'en');",
    },
    {
      filename: featureFile,
      code: 'function withPolyfill(Intl) { return new Intl.NumberFormat(); }',
    },
    {
      filename: featureFile,
      code: 'function withSandbox(globalThis) { return globalThis.Intl.NumberFormat(); }',
    },
    {
      filename: featureFile,
      code: 'type NumberOptions = Intl.NumberFormatOptions;',
      languageOptions: {
        parser: tseslint.parser,
      },
    },
    {
      filename: featureFile,
      code: 'type IntlRuntime = typeof Intl;',
      languageOptions: {
        parser: tseslint.parser,
      },
    },
  ],
  invalid: [
    {
      filename: featureFile,
      code: "const segmenter = new Intl.Segmenter('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const segmenter = new globalThis.Intl['Segmenter']('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const formatter = new Intl.NumberFormat('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const canonical = Intl.getCanonicalLocales('EN-us');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const formatter = Intl.NumberFormat('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: 'const Formatter = Intl.NumberFormat;',
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const formatter = new Intl['DateTimeFormat']('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: 'const formatter = new Intl[name]();',
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: "const formatter = new globalThis.Intl.ListFormat('en');",
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: 'const NativeIntl = globalThis.Intl;',
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: 'const { PluralRules } = Intl;',
      errors: [{ messageId: 'directIntl' }],
    },
    {
      filename: featureFile,
      code: 'value.toLocaleString();',
      errors: [{ messageId: 'localeMethod' }],
    },
    {
      filename: featureFile,
      code: "value['toLocaleDateString']();",
      errors: [{ messageId: 'localeMethod' }],
    },
    {
      filename: featureFile,
      code: "import { createNumberFormatter } from '@shared/intl';",
      errors: [{ messageId: 'directIntlImport' }],
    },
    {
      filename: featureFile,
      code: "import { presets } from '@/shared/intl/internal';",
      errors: [{ messageId: 'directIntlImport' }],
    },
    {
      filename: featureFile,
      code: "export { createNumberFormatter } from '@shared/intl';",
      errors: [{ messageId: 'directIntlImport' }],
    },
    {
      filename: featureFile,
      code: "export * from '@/shared/intl/internal';",
      errors: [{ messageId: 'directIntlImport' }],
    },
    {
      filename: featureFile,
      code: "const registry = import('@shared/intl');",
      errors: [{ messageId: 'directIntlImport' }],
    },
  ],
});
