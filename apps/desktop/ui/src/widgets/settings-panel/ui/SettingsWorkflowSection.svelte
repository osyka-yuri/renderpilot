<script lang="ts">
  import { Badge, Switch } from '@shared/ui';
  import SettingsSectionShell from './SettingsSectionShell.svelte';
  import SettingRow from './SettingRow.svelte';
  import { cn } from '@shared/classnames';

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

  const { advancedMode = false, onToggleAdvancedMode = () => undefined }: Props = $props();

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
  <SettingRow>
    <div class="grid min-w-0 flex-1 gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
        {advancedModeCopy.label}
      </p>
      <span id={ADVANCED_MODE_LABEL_ID} class="text-base/5 font-semibold text-foreground">
        {advancedModeCopy.title}
      </span>
      <span id={ADVANCED_MODE_DESCRIPTION_ID} class="text-sm/snug">
        {advancedModeCopy.description}
      </span>
    </div>

    <Switch
      checked={advancedMode}
      aria-labelledby={ADVANCED_MODE_LABEL_ID}
      aria-describedby={ADVANCED_MODE_DESCRIPTION_ID}
      onCheckedChange={handleAdvancedModeChange}
    />
  </SettingRow>

  <SettingRow>
    <div class="grid min-w-0 flex-1 gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
        {scanSourceCopy.label}
      </p>
      <h4>{scanSourceCopy.title}</h4>
      <p>{scanSourceCopy.description}</p>
    </div>

    <div
      class={cn(
        'grid min-w-max shrink-0 justify-items-end gap-2',
        'max-md:min-w-0 max-md:justify-items-start',
      )}
      aria-label={scanSourceCopy.note}
    >
      <Badge variant="outline">{scanSourceCopy.badge}</Badge>
      <span class="text-xs/snug text-muted-foreground">{scanSourceCopy.note}</span>
    </div>
  </SettingRow>
</SettingsSectionShell>
