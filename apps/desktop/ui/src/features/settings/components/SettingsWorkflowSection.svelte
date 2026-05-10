<script lang="ts">
  import Badge from '@shared/ui/Badge.svelte';
  import Switch from '@shared/ui/Switch.svelte';
  import SettingsSectionShell from '@features/settings/components/SettingsSectionShell.svelte';

  type ToggleAdvancedModeHandler = () => void;

  const SECTION_TITLE_ID = 'workflow-title';
  const ADVANCED_MODE_LABEL_ID = 'advanced-mode-label';
  const ADVANCED_MODE_DESCRIPTION_ID = 'advanced-mode-description';

  const sectionCopy = {
    eyebrow: 'Behavior',
    title: 'Workflow and provider posture',
    description:
      'Keep operational behavior predictability and expose lower-level controls only when they improve the workflow.',
  } as const;

  const advancedModeCopy = {
    label: 'Detail level',
    title: 'Advanced mode',
    description:
      'Show lower-level actions and technical controls in detail screens only when you need them.',
  } as const;

  const scanSourceCopy = {
    label: 'Discovery',
    title: 'Scan source',
    description:
      'Manual folder scanning is active. Provider integrations can be added later without changing the overall settings hierarchy.',
    badge: 'Manual scan',
    note: 'Current source',
  } as const;

  type Props = {
    advancedMode?: boolean;
    onToggleAdvancedMode?: ToggleAdvancedModeHandler;
  };

  let { advancedMode = false, onToggleAdvancedMode = () => undefined }: Props = $props();

  function handleAdvancedModeChange(nextChecked: boolean): void {
    if (nextChecked === advancedMode) {
      return;
    }

    onToggleAdvancedMode();
  }
</script>

<SettingsSectionShell
  titleId={SECTION_TITLE_ID}
  eyebrow={sectionCopy.eyebrow}
  title={sectionCopy.title}
  description={sectionCopy.description}
>
  <div class="setting-row switch-row">
    <Switch
      checked={advancedMode}
      aria-labelledby={ADVANCED_MODE_LABEL_ID}
      aria-describedby={ADVANCED_MODE_DESCRIPTION_ID}
      onCheckedChange={handleAdvancedModeChange}
    >
      <span class="setting-copy">
        <span class="setting-label">{advancedModeCopy.label}</span>
        <span id={ADVANCED_MODE_LABEL_ID} class="row-title">
          {advancedModeCopy.title}
        </span>
        <span id={ADVANCED_MODE_DESCRIPTION_ID} class="row-copy">
          {advancedModeCopy.description}
        </span>
      </span>
    </Switch>
  </div>

  <div class="setting-row status-row">
    <div class="setting-copy">
      <p class="setting-label">{scanSourceCopy.label}</p>
      <h4>{scanSourceCopy.title}</h4>
      <p>{scanSourceCopy.description}</p>
    </div>

    <div class="setting-status" aria-label={scanSourceCopy.note}>
      <Badge pill size="md" tone="muted">{scanSourceCopy.badge}</Badge>
      <span class="status-note">{scanSourceCopy.note}</span>
    </div>
  </div>
</SettingsSectionShell>

<style>
  .switch-row {
    padding-block: var(--space-4);
  }

  .status-row {
    align-items: flex-start;
  }

  .setting-status {
    display: grid;
    flex-shrink: 0;
    gap: var(--space-2);
    justify-items: end;
    min-inline-size: max-content;
  }

  .status-note {
    color: var(--text-muted);
    font-size: 0.74rem;
    line-height: 1.35;
  }

  @media (max-width: 720px) {
    .setting-status {
      justify-items: start;
      min-inline-size: 0;
    }
  }
</style>
