# Accessibility

RenderPilot targets WCAG 2.2 AA for its desktop interface. Accessibility is a release contract: localized accessible names, keyboard behavior, focus management, screen-reader announcements, reflow, contrast, forced colors, and reduced motion must remain usable in every supported locale.

## Automated checks

Run the browser accessibility matrix from `apps/desktop`:

```powershell
pnpm exec playwright install chromium
pnpm run test:a11y
```

The retained suite runs axe WCAG 2.2 A/AA checks and focused behavioral assertions against the Chromium browser preview. It covers the English shell, overlays, menus, executable selection, mobile navigation and filters, real launcher keyboard and pointer reorder (including two-Escape behavior), Libraries caption/sort/action feedback, one focused Russian DnD localization flow, representative light/dark layering, reduced motion, forced colors, and 320 CSS px bounds. A compact locale pass checks language, nonempty landmarks, one Games axe scan, and document bounds for `en`, `ru`, `es`, `fr`, `de`, `ja`, `zh-Hans`, and `zh-Hant`. CI installs Chromium and runs this browser coverage as part of the reusable quality workflow. It does not validate the native Tauri WebView2 host or Windows screen-reader integration.

ESLint rejects nonblank hardcoded values in user-facing `aria-label`, `aria-description`, `aria-placeholder`, `aria-roledescription`, `aria-valuetext`, `placeholder`, `title`, and `alt` attributes. This includes literal text, static template/concatenation fragments, and hardcoded branches of conditional or logical expressions; arbitrary calls and translated/dynamic values remain unresolved and are allowed. Empty `alt` remains valid for decorative images.

The rule also rejects hardcoded Svelte text and statically provable mustache text inside containers that are guaranteed to be screen-reader-only. It recognizes plain `sr-only` classes and bounded `cn`, `clsx`, and `cx` compositions, including class arrays, object maps, conditional branches, spreads, and later overrides. `not-sr-only`, unknown class inputs, and contradictory or dynamic class paths are not treated as guaranteed screen-reader-only. Product layers are forbidden from importing `bits-ui` directly: reusable primitives belong to the `@shared/ui` public API. Notification dispatch goes through `@shared/notifications`; direct `svelte-sonner` imports are limited to its shared Toaster and notification adapter. The typed localization catalog requires every retained accessibility message in every locale.

Automated browser checks complement rather than replace the packaged Windows release gate below. Axe cannot determine whether focus is visually obvious, announcements are well timed, or speech is understandable in context.

## Required Windows screen-reader matrix

Run this release gate against a packaged Tauri build on Windows 11 with WebView2 136 or newer. Test both the latest stable NVDA and Windows Narrator. Record the application version, WebView2 version, screen-reader version, tester, date, locale, observed speech/focus, result, and defect link for every run.

Each screen-reader and locale combination must cover:

1. Start the application and confirm the localized document title, one page heading, and no duplicate loading announcement.
2. Use the skip link, primary navigation, breadcrumb links, and browser-style route changes. Confirm visible focus, the current-page state, main-content focus, and the new page name.
3. On Games, use search and quick filters; open and operate the game actions menu with Enter, arrows, and Escape; confirm focus returns to its trigger.
4. Open Filters and reorder a launcher using Space or Enter, arrows, drop, and Escape cancellation. Confirm localized pickup, position, drop, and cancellation announcements.
5. Open Details, switch tabs, operate Update all, and select a game executable as a radio group. Confirm the checked option and trigger focus after closing.
6. Exercise every Settings tab, select, switch, validation error, asynchronous status, and update-progress phase. Percent changes must not produce continuous speech.
7. On Libraries, switch vendor/type, sort columns, read the table caption and sort state, invoke version-specific actions, and open/close the legal-documents sheet.
8. Check Operations empty and populated states plus one representative dialog, warning, error, toast, and progress operation. An event must not be announced twice.

Repeat the flow in light and dark themes. For each locale, also verify Windows High Contrast, `Reduce motion`, the 960×720 minimum window, the 1280×900 default window, 200% and 400% zoom, and custom text spacing. At 320 CSS px equivalent width, the document must not scroll horizontally; a wide data table may scroll only inside its own labeled container.

## Acceptance and defect policy

- All automated accessibility checks must pass without blanket axe exclusions.
- Every NVDA and Narrator matrix cell must pass in all eight locales.
- Focus indicators must have at least a 2 CSS px equivalent area and 3:1 contrast in light, dark, and forced-colors modes.
- Interactive targets must be at least 24×24 CSS px.
- Meaning must never depend on color, animation, pointer input, or visual iconography alone.
- Accessibility defects block release. Any temporary rule exception must identify a tracked defect, owner, narrow target, and expiry; permanent broad allowlists are not accepted.

## Sources of truth

- [Desktop package manifest](../../apps/desktop/package.json)
- [Playwright configuration](../../apps/desktop/playwright.config.ts)
- [Quality workflow](../../.github/workflows/quality.yml)
- [Localization guide](localization.md)
