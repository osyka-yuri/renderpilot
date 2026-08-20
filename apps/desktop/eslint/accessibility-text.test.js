import { RuleTester } from 'eslint';
import { describe, it } from 'vitest';
import svelteParser from 'svelte-eslint-parser';

import { noHardcodedAccessibilityTextRule } from './accessibility-text.js';

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

const ruleTester = new RuleTester({
  languageOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
    parser: svelteParser,
  },
});

ruleTester.run('no-hardcoded-accessibility-text', noHardcodedAccessibilityTextRule, {
  valid: [
    "<button aria-label={t('common.close')}>×</button>",
    "<input aria-valuetext={t('progress.value', { percent })} />",
    '<button aria-label={label}>×</button>',
    '<input aria-label={` ${label}`}>',
    '<button aria-label={`${name}`}>×</button>',
    '<img alt="" src="decorative.svg" />',
    '<span class="sr-only">{t(\'common.close\')}</span>',
    "<span class={'sr-only'}>{t('common.close')}</span>",
    '<span class={`sr-only`}>{label}</span>',
    '<span class:sr-only={hidden}>Visible text</span>',
    "<span class={hidden ? 'sr-only' : 'visually-hidden'}>Close</span>",
    "<span class={hidden && 'sr-only'}>Close</span>",
    "<span class={cn(hidden && 'sr-only')}>Close</span>",
    "<span class={{ 'sr-only': hidden }}>Close</span>",
    '<span class="sr-only not-sr-only">Close</span>',
    '<span class="sr-only">{label}</span>',
    "<span class={{ ...classes, 'sr-only': true }}>Close</span>",
    '<span class:sr-only={false}>Close</span>',
    '<span class:sr-only={hidden}>Close</span>',
    '<span class:not-sr-only={true}>Close</span>',
    '<span class:not-sr-only={hidden}>Close</span>',
  ],
  invalid: [
    {
      code: '<button aria-label="Close">×</button>',
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: "<button aria-label={'Close'}>×</button>",
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: '<button aria-label={`Close ${label}`}>×</button>',
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: "<button aria-label={'Close ' + label}>×</button>",
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: '<button aria-label="Close {label}">×</button>',
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: "<button aria-label={name ? 'Close' : t('common.close')}>×</button>",
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: "<button aria-label={name && 'Close'}>×</button>",
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: '<input placeholder="Search games" />',
      errors: [{ messageId: 'hardcodedAttribute' }],
    },
    {
      code: '<span class="sr-only">Close</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only emphasis">{\'Close\'}</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only"><strong>Close</strong></span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only">{`Close ${name}`}</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only">{\'Close \' + name}</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class=\"sr-only\">{name ? 'Close' : t('common.close')}</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only">{name && \'Close\'}</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={'sr-only'}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class={`sr-only`}>Close</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={'sr-' + 'only'}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={cn('sr-only', hidden && 'text-xs')}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={clsx('sr-only', compact && 'text-xs')}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={cx('sr-only', compact && 'text-xs')}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={['sr-only', compact && 'text-xs']}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={{ 'sr-only': true, 'text-xs': compact }}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: "<span class={compact ? 'sr-only text-xs' : 'sr-only'}>Close</span>",
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class:sr-only={true}>Close</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
    {
      code: '<span class="sr-only" class:not-sr-only={false}>Close</span>',
      errors: [{ messageId: 'hardcodedSrOnly' }],
    },
  ],
});
