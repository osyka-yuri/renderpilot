<script lang="ts">
  import type { LanguageModeHandler, ThemeModeHandler, VoidHandler } from '@shared/utils/callbacks';
  import type { ThemeMode } from '@shared/theme/theme-mode';

  import Badge from '@shared/ui/Badge.svelte';
  import Select from '@shared/ui/Select.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import Switch from '@shared/ui/Switch.svelte';

  type SelectOption<Value extends string> = {
    value: Value;
    label: string;
  };

  const themeOptions = [
    { value: 'system', label: 'System' },
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' },
  ] satisfies SelectOption<ThemeMode>[];

  const languageOptions = [
    { value: 'system', label: 'Follow system' },
    { value: 'en', label: 'English' },
    { value: 'ru', label: 'Russian' },
  ] satisfies SelectOption<'system' | 'en' | 'ru'>[];

  type LanguageMode = (typeof languageOptions)[number]['value'];

  const noopThemeChange: ThemeModeHandler = () => undefined;
  const noopLanguageChange: LanguageModeHandler = () => undefined;
  const noopToggle: VoidHandler = () => undefined;

  export let themeMode: ThemeMode = 'system';
  export let languageMode: LanguageMode = 'system';
  export let advancedMode = false;

  export let onThemeModeChange: ThemeModeHandler = noopThemeChange;
  export let onLanguageModeChange: LanguageModeHandler = noopLanguageChange;
  export let onToggleAdvancedMode: VoidHandler = noopToggle;

  function isOptionValue<Value extends string>(
    value: string,
    options: SelectOption<Value>[],
  ): value is Value {
    return options.some((option) => option.value === value);
  }

  function handleThemeChange(nextValue: string): void {
    if (!isOptionValue(nextValue, themeOptions)) {
      return;
    }

    onThemeModeChange(nextValue);
  }

  function handleLanguageChange(nextValue: string): void {
    if (!isOptionValue(nextValue, languageOptions)) {
      return;
    }

    onLanguageModeChange(nextValue);
  }

  function handleAdvancedModeToggle(): void {
    onToggleAdvancedMode();
  }
</script>

<section class="screen-shell" aria-label="Settings">
  <article class="settings-section" aria-labelledby="appearance-title">
    <header class="section-header">
      <p class="eyebrow">Interface</p>
      <h3 id="appearance-title">Appearance and language</h3>
      <p class="section-copy">
        Keep the shell visually consistent across themes and languages without turning preferences
        into oversized cards.
      </p>
    </header>

    <Surface className="settings-panel" tone="elevated" shadow>
      <div class="setting-row">
        <div class="setting-copy">
          <p class="setting-label">Display</p>
          <h4>Theme</h4>
          <p>
            Follow the operating system appearance or choose a fixed theme while keeping the
            application palette internally consistent.
          </p>
        </div>

        <span class="setting-control">
          <Select
            ariaLabel="Theme mode"
            options={themeOptions}
            value={themeMode}
            onValueChange={handleThemeChange}
          />
        </span>
      </div>

      <div class="setting-row">
        <div class="setting-copy">
          <p class="setting-label">Localization</p>
          <h4>Language</h4>
          <p>
            Use a scalable selector so more interface languages can be added later without changing
            the page structure.
          </p>
        </div>

        <span class="setting-control">
          <Select
            ariaLabel="Interface language"
            options={languageOptions}
            value={languageMode}
            onValueChange={handleLanguageChange}
          />
        </span>
      </div>
    </Surface>
  </article>

  <article class="settings-section" aria-labelledby="workflow-title">
    <header class="section-header">
      <p class="eyebrow">Behavior</p>
      <h3 id="workflow-title">Workflow and provider posture</h3>
      <p class="section-copy">
        Keep operational behavior predictable and expose lower-level controls only when they improve
        the workflow.
      </p>
    </header>

    <Surface className="settings-panel" tone="elevated" shadow>
      <div class="setting-row switch-row">
        <Switch
          checked={advancedMode}
          aria-label="Advanced mode"
          onclick={handleAdvancedModeToggle}
        >
          <span class="setting-copy">
            <span class="setting-label">Detail level</span>
            <span class="row-title">Advanced mode</span>
            <span class="row-copy">
              Show lower-level actions and technical controls in detail screens only when you need
              them.
            </span>
          </span>
        </Switch>
      </div>

      <div class="setting-row status-row">
        <div class="setting-copy">
          <p class="setting-label">Discovery</p>
          <h4>Scan source</h4>
          <p>
            Manual folder scanning is active. Provider integrations can be added later without
            changing the overall settings hierarchy.
          </p>
        </div>

        <div class="setting-status">
          <Badge pill size="md" tone="muted">Manual scan</Badge>
          <span class="status-note">Current source</span>
        </div>
      </div>
    </Surface>
  </article>
</section>

<style>
  .screen-shell,
  .settings-section,
  .section-header,
  .setting-copy,
  .setting-status {
    display: grid;
  }

  .screen-shell {
    gap: var(--space-5);
    width: 100%;
  }

  .settings-section {
    gap: var(--space-3);
  }

  .section-header {
    gap: var(--space-1);
    padding-inline: var(--space-1);
  }

  .eyebrow,
  .setting-label {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h3,
  h4 {
    margin: 0;
    font-weight: 600;
  }

  h3 {
    font-size: 1.05rem;
  }

  h4,
  .row-title {
    color: var(--text-strong);
    font-size: 0.95rem;
    font-weight: 600;
  }

  .section-copy {
    max-width: 56rem;
    margin: 0;
    font-size: 0.875rem;
    line-height: 1.45;
  }

  :global(.settings-panel) {
    display: grid;
    gap: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
  }

  :global(.settings-panel) > :last-child {
    border-bottom: 0;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  .setting-copy {
    flex: 1;
    min-width: 0;
    gap: var(--space-1);
  }

  .setting-copy p,
  .row-copy {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .setting-control {
    display: block;
    width: min(100%, 15rem);
    min-width: 13rem;
    flex-shrink: 0;
  }

  .switch-row {
    padding-block: var(--space-4);
  }

  .status-row {
    align-items: flex-start;
  }

  .setting-status {
    flex-shrink: 0;
    justify-items: end;
    gap: var(--space-2);
  }

  .status-note {
    color: var(--text-muted);
    font-size: 0.74rem;
  }

  @media (max-width: 720px) {
    .section-header {
      padding-inline: 0;
    }

    .setting-row {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .setting-control {
      width: 100%;
      min-width: 0;
    }

    .setting-status {
      justify-items: start;
    }
  }
</style>
