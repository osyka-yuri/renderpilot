<script lang="ts">
  import type { LanguageModeHandler, ThemeModeHandler, VoidHandler } from '@shared/utils/callbacks';
  import type { ThemeMode } from '@shared/theme/theme-mode';
  import Badge from '@shared/ui/Badge.svelte';
  import Select from '@shared/ui/Select.svelte';
  import Surface from '@shared/ui/Surface.svelte';
  import Switch from '@shared/ui/Switch.svelte';

  type LanguageMode = 'system' | 'en' | 'ru';

  const themeOptions: Array<{ value: ThemeMode; label: string }> = [
    { value: 'system', label: 'System' },
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' },
  ];
  const languageOptions: Array<{ value: LanguageMode; label: string }> = [
    { value: 'system', label: 'Follow system' },
    { value: 'en', label: 'English' },
    { value: 'ru', label: 'Russian' },
  ];
  const noopTheme: ThemeModeHandler = (_mode: ThemeMode): void => {};
  const noopLanguage: LanguageModeHandler = (_mode: LanguageMode): void => {};
  const noopToggle: VoidHandler = (): void => {};

  export let themeMode: ThemeMode = 'system';
  export let languageMode: LanguageMode = 'system';
  export let advancedMode = false;
  export let onThemeModeChange: ThemeModeHandler = noopTheme;
  export let onLanguageModeChange: LanguageModeHandler = noopLanguage;
  export let onToggleAdvancedMode: VoidHandler = noopToggle;

  function handleThemeChange(nextValue: string): void {
    onThemeModeChange(nextValue as ThemeMode);
  }

  function handleLanguageChange(nextValue: string): void {
    onLanguageModeChange(nextValue as LanguageMode);
  }
</script>

<section class="screen-shell">
  <article class="settings-section">
    <div class="section-header">
      <p class="eyebrow">Interface</p>
      <h3>Appearance and language</h3>
      <p class="section-copy">Keep the shell visually consistent across themes and languages without turning preferences into oversized cards.</p>
    </div>

    <Surface className="settings-panel" tone="elevated" shadow>
      <div class="setting-row select-row">
        <div class="setting-copy">
          <p class="setting-label">Display</p>
          <h4>Theme</h4>
          <p>Follow the operating system appearance or choose a fixed theme while keeping the application palette internally consistent.</p>
        </div>

        <span class="setting-control select-wrap">
          <Select
            ariaLabel="Theme mode"
            options={themeOptions}
            value={themeMode}
            onValueChange={handleThemeChange}
          />
        </span>
      </div>

      <div class="setting-row select-row">
        <div class="setting-copy">
          <p class="setting-label">Localization</p>
          <h4>Language</h4>
          <p>Use a scalable selector so more interface languages can be added later without changing the page structure.</p>
        </div>

        <span class="setting-control select-wrap">
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

  <article class="settings-section">
    <div class="section-header">
      <p class="eyebrow">Behavior</p>
      <h3>Workflow and provider posture</h3>
      <p class="section-copy">Keep operational behavior predictable and expose lower-level controls only when they improve the workflow.</p>
    </div>

    <Surface className="settings-panel" tone="elevated" shadow>
      <div class="setting-row switch-row">
        <Switch checked={advancedMode} ariaLabel="Advanced mode" onclick={onToggleAdvancedMode}>
          <span class="setting-copy">
            <span class="setting-label">Detail level</span>
            <span class="row-title">Advanced mode</span>
            <span class="row-copy">Show lower-level actions and technical controls in detail screens only when you need them.</span>
          </span>
        </Switch>
      </div>

      <div class="setting-row status-row">
        <div class="setting-copy">
          <p class="setting-label">Discovery</p>
          <h4>Scan source</h4>
          <p>Manual folder scanning is active. Provider integrations can be added later without changing the overall settings hierarchy.</p>
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
  .screen-shell {
    display: grid;
    gap: var(--space-5);
    width: 100%;
  }

  .settings-section {
    display: grid;
    gap: var(--space-3);
  }

  .eyebrow {
    margin: 0;
    color: var(--text-subtle);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.6875rem;
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
  }

  h4 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text-strong);
  }

  .section-header {
    display: grid;
    gap: var(--space-1);
    padding: 0 var(--space-1);
  }

  .section-copy {
    margin: 0;
    max-width: 56rem;
    font-size: 0.875rem;
    line-height: 1.45;
  }

  :global(.settings-panel) {
    display: grid;
    gap: 0;
    overflow: hidden;
    border-radius: var(--radius-xl);
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  :global(.settings-panel) > :last-child {
    border-bottom: 0;
  }

  .setting-copy {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    flex: 1;
  }

  .setting-label {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .setting-copy p,
  .row-copy {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .select-row {
    cursor: default;
  }

  .setting-control {
    width: min(100%, 15rem);
    min-width: 13rem;
    flex-shrink: 0;
  }

  .select-wrap {
    display: block;
  }

  .row-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text-strong);
  }

  .switch-row {
    padding-block: var(--space-4);
  }

  .status-row {
    align-items: flex-start;
  }

  .setting-status {
    display: grid;
    justify-items: end;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .status-note {
    color: var(--text-muted);
    font-size: 0.74rem;
  }

  @media (max-width: 720px) {
    .setting-row {
      flex-direction: column;
      align-items: stretch;
      gap: 0.75rem;
    }

    .section-header {
      padding: 0;
    }

    .setting-control,
    .select-wrap {
      min-width: 0;
      width: 100%;
    }

    .setting-status {
      justify-items: start;
    }
  }
</style>