# Localization

The desktop has eight effective locales: English plus Russian, German, Spanish, French, Japanese, Simplified Chinese, and Traditional Chinese. English is the structural source. The seven non-English locale packs are lazy modules loaded on demand together with their Luma/NVAPI translations and locale-neutral source contracts, so those external catalogs remain outside the initial bundle graph.

## Generated contracts

Run localization generation whenever English message structure, a Luma source contract, the bundled NVAPI catalog, editorial policy, or either error/warning manifest changes:

```powershell
cd apps/desktop
pnpm run i18n:generate
```

`pnpm run i18n:check` confirms generated files are current. `pnpm test` includes the message, bundle-boundary, source-contract, and review-tool tests. `pnpm build` repeats contract generation checks through type checking before Vite and then validates the bundle graph.

The editorial policy is the source for official names, protected literals, high-risk concepts, punctuation and typography rules, and Simplified/Traditional Chinese exclusions. Do not fix a protected product name by adding an unreviewed per-locale exception.

## External Luma and NVAPI sources

Luma and NVAPI strings are source-bound. The normalized Luma records contain `key`, exact `sourceText`, and messages shaped as `{ id, context }`; the Luma contract deliberately stores neither a producer repository SHA nor a timestamp. The bundled NVAPI snapshot covers the supported `sr`, `fg`, and `rr` projection. When a localized override no longer matches the current English source, presentation uses current English rather than a stale translation. Verify the checked-out producer before regeneration:

```powershell
cd apps/desktop
pnpm run i18n:source-check --producer-root ../../../renderpilot-libraries
pnpm run i18n:generate
```

The source path is expected to contain the producer's versioned Luma add-on manifest. CI pins the producer repository to a full commit rather than following a moving branch.

## Review workflow

Generate a locale report in the format most useful to the reviewer:

```powershell
cd apps/desktop
pnpm run i18n:review --locale ru --format tsv
pnpm run i18n:review --locale ja --format json
```

The current external review set contains 269 rows: 99 Luma messages and 170 NVAPI messages. Repeat review for all seven non-English locales after relevant source changes. A clean generated report and passing automated checks establish structural consistency; they do not constitute native-language approval.

Use stable localization IDs in add-on catalogs and retain a reviewed English fallback for every published message. Keep interpolation typed and structural. Do not concatenate translated fragments or let untrusted manifest prose become an implicit localization key.

## Sources of truth

- [Editorial policy](../../apps/desktop/data/i18n-editorial-policy.json)
- [Localization generator](../../apps/desktop/scripts/generate-i18n-contracts.mjs)
- [Review tool](../../apps/desktop/scripts/i18n-review.mjs)
- [Locale pack registry](../../apps/desktop/ui/src/shared/i18n/packs/registry.ts)
