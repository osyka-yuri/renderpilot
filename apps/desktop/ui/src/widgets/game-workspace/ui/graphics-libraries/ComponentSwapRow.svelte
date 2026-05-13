<script lang="ts">
  import {
    candidateOptionsForRow,
    installedOptionsForRow,
    type ConfiguredComponentRow,
  } from '@features/graphics-configurator';
  import { formatLabel } from '@entities/component';
  import { riskBadgeVariant } from '@entities/operation';
  import { cn } from '@shared/classnames';
  import { fileNameFromPath } from '@shared/path';
  import {
    Badge,
    Button,
    Card,
    CardContent,
    CardFooter,
    CardHeader,
    CardTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from '@shared/ui';

  type ArtifactSelectionHandler = (componentId: string, value: string) => void;
  type BuildPlanHandler = (componentId: string, artifactId: string) => void;

  type Props = {
    row: ConfiguredComponentRow;
    selectedArtifact?: string;
    riskLevel?: string | null | undefined;
    busy?: boolean;
    onArtifactSelection?: ArtifactSelectionHandler;
    onBuildPlan?: BuildPlanHandler;
  };

  const {
    row,
    selectedArtifact = '',
    riskLevel = null,
    busy = false,
    onArtifactSelection = () => undefined,
    onBuildPlan = () => undefined,
  }: Props = $props();

  type SelectedCandidate = ConfiguredComponentRow['selectedCandidate'];

  type RowViewModel = {
    componentId: string;
    currentPath: string;
    displayPath: string;
    fileName: string;
    installedValue: string;
    replacementValue: string;
    installedOptions: ReturnType<typeof installedOptionsForRow>;
    candidateOptions: ReturnType<typeof candidateOptionsForRow>;
    hasCandidates: boolean;
    selectedCandidate: SelectedCandidate;
    selectedArtifactId: string | undefined;
    replacementSelectDisabled: boolean;
    buildPlanDisabled: boolean;
    compatibilityLabel: string;
    candidatePath: string;
    candidateSummary: string;
    selectionSummaryTitle: string;
    installedSelectLabel: string;
    replacementSelectLabel: string;
  };

  const EMPTY_VALUE = '';
  const FALLBACK_TEXT = '—';
  const NO_REPLACEMENTS_TEXT = 'No replacement candidates found';
  const SELECT_REPLACEMENT_TEXT = 'Choose a replacement version';
  const UNKNOWN_PATH_TEXT = 'Path unavailable';

  type SelectOption = {
    value: string;
    label: string;
    disabled?: boolean;
  };

  function displayText(value: string | null | undefined, fallback = FALLBACK_TEXT) {
    return value?.trim() ? value : fallback;
  }

  function compatibleVersionsLabel(count: number) {
    return count === 1 ? '1 compatible version' : `${count} compatible versions`;
  }

  function resolveSelectedArtifactId(
    nextRow: ConfiguredComponentRow,
    nextSelectedArtifact: string,
  ) {
    return nextSelectedArtifact || nextRow.selectedCandidate?.artifact_id;
  }

  function resolveSelectedCandidate(
    nextRow: ConfiguredComponentRow,
    selectedArtifactId: string | undefined,
  ): SelectedCandidate {
    if (!selectedArtifactId) {
      return null;
    }

    if (nextRow.selectedCandidate?.artifact_id === selectedArtifactId) {
      return nextRow.selectedCandidate;
    }

    return (
      nextRow.group?.candidates.find((candidate) => candidate.artifact_id === selectedArtifactId) ??
      null
    );
  }

  function replacementHelperText(
    nextRow: ConfiguredComponentRow,
    hasCandidates: boolean,
    selectedArtifactId: string | undefined,
  ) {
    if (!hasCandidates) {
      return NO_REPLACEMENTS_TEXT;
    }

    if (!selectedArtifactId) {
      return SELECT_REPLACEMENT_TEXT;
    }

    return displayText(nextRow.candidatePath, UNKNOWN_PATH_TEXT);
  }

  function replacementSummaryText(
    nextRow: ConfiguredComponentRow,
    selectedCandidate: SelectedCandidate,
  ) {
    return displayText(
      nextRow.candidateSummary,
      selectedCandidate ? 'Replacement details unavailable' : 'No replacement selected',
    );
  }

  function buildRowViewModel(
    nextRow: ConfiguredComponentRow,
    nextSelectedArtifact: string,
    nextBusy: boolean,
  ): RowViewModel {
    const componentId = nextRow.component.id;
    const currentPath = displayText(nextRow.currentInstalled.path, UNKNOWN_PATH_TEXT);
    const displayPath = displayText(nextRow.group?.file_path, currentPath);
    const candidatesCount = nextRow.group?.candidates.length ?? 0;
    const hasCandidates = candidatesCount > 0;

    const selectedArtifactId = resolveSelectedArtifactId(nextRow, nextSelectedArtifact);
    const selectedCandidate = resolveSelectedCandidate(nextRow, selectedArtifactId);

    const canBuildPlan = Boolean(nextRow.canBuildPlan && selectedArtifactId && selectedCandidate);

    const fileName = displayText(fileNameFromPath(currentPath), currentPath);

    return {
      componentId,
      currentPath,
      displayPath,
      fileName,
      installedValue: nextRow.installedValue,
      replacementValue: selectedArtifactId ?? EMPTY_VALUE,
      installedOptions: installedOptionsForRow(nextRow),
      candidateOptions: candidateOptionsForRow(nextRow),
      hasCandidates,
      selectedCandidate,
      selectedArtifactId,
      replacementSelectDisabled: nextBusy || !hasCandidates,
      buildPlanDisabled: nextBusy || !canBuildPlan,
      compatibilityLabel: compatibleVersionsLabel(candidatesCount),
      candidatePath: replacementHelperText(nextRow, hasCandidates, selectedArtifactId),
      candidateSummary: replacementSummaryText(nextRow, selectedCandidate),
      selectionSummaryTitle: selectedCandidate ? 'Selected replacement' : 'Replacement selection',
      installedSelectLabel: `Installed version for ${fileName}`,
      replacementSelectLabel: `Replacement version for ${fileName}`,
    };
  }

  const view = $derived(buildRowViewModel(row, selectedArtifact, busy));

  function handleArtifactSelection(value: string) {
    if (view.replacementSelectDisabled || value === view.replacementValue) {
      return;
    }

    onArtifactSelection(view.componentId, value);
  }

  function handleBuildPlan() {
    if (view.buildPlanDisabled || !view.selectedArtifactId) {
      return;
    }

    onBuildPlan(view.componentId, view.selectedArtifactId);
  }

  function optionLabel(options: readonly SelectOption[], value: string, fallback: string): string {
    return options.find((option) => option.value === value)?.label ?? fallback;
  }

  const installedTriggerLabel = $derived(
    optionLabel(view.installedOptions, view.installedValue, 'No detected file'),
  );

  const replacementTriggerLabel = $derived(
    optionLabel(view.candidateOptions, view.replacementValue, SELECT_REPLACEMENT_TEXT),
  );
</script>

<Card>
  <CardHeader>
    <div
      class={cn('flex items-start justify-between gap-4', 'max-lg:flex-col max-lg:items-stretch')}
    >
      <div class="grid min-w-0 gap-1">
        <CardTitle>{view.fileName}</CardTitle>
        <p class="min-w-0 text-sm/5 break-all text-muted-foreground" title={view.displayPath}>
          {view.displayPath}
        </p>
      </div>

      <div class="flex flex-wrap justify-end gap-2" aria-label="Compatibility information">
        <Badge variant={riskBadgeVariant(riskLevel)}>
          {formatLabel(row.component.swappability)}
        </Badge>

        {#if view.hasCandidates}
          <Badge>{view.compatibilityLabel}</Badge>
        {:else}
          <Badge variant="secondary">No replacements</Badge>
        {/if}
      </div>
    </div>
  </CardHeader>

  <CardContent>
    <div class="grid min-w-0 gap-2">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
        Installed version
      </p>

      <Select type="single" items={view.installedOptions} value={view.installedValue} disabled>
        <SelectTrigger size="sm" class="w-full" aria-label={view.installedSelectLabel}>
          {installedTriggerLabel}
        </SelectTrigger>
        <SelectContent>
          {#each view.installedOptions as option (option.value)}
            <SelectItem value={option.value} label={option.label}>
              {option.label}
            </SelectItem>
          {/each}
        </SelectContent>
      </Select>

      <small
        class="block min-w-0 text-xs/snug break-all text-muted-foreground"
        title={view.currentPath}>{view.currentPath}</small
      >
    </div>

    <div class="grid min-w-0 gap-2">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
        Replacement version
      </p>

      <Select
        type="single"
        items={view.candidateOptions}
        value={view.replacementValue}
        disabled={view.replacementSelectDisabled}
        onValueChange={handleArtifactSelection}
      >
        <SelectTrigger size="sm" class="w-full" aria-label={view.replacementSelectLabel}>
          {replacementTriggerLabel}
        </SelectTrigger>
        <SelectContent>
          {#each view.candidateOptions as option (option.value)}
            <SelectItem value={option.value} label={option.label} disabled={option.disabled}>
              {option.label}
            </SelectItem>
          {/each}
        </SelectContent>
      </Select>

      <small
        class="block min-w-0 text-xs/snug break-all text-muted-foreground"
        title={view.candidatePath}>{view.candidatePath}</small
      >
    </div>
  </CardContent>

  <CardFooter>
    <div class="grid max-w-176 min-w-0 gap-1">
      <strong class="text-foreground">{view.selectionSummaryTitle}</strong>
      <p class="text-sm/5 wrap-break-word text-muted-foreground">
        {view.candidateSummary}
      </p>
    </div>

    <Button variant="default" size="sm" disabled={view.buildPlanDisabled} onclick={handleBuildPlan}>
      {busy ? 'Working...' : 'Build File Plan'}
    </Button>
  </CardFooter>
</Card>
