<script lang="ts">
  import {
    candidateOptionsForRow,
    installedOptionsForRow,
    type ConfiguredComponentRow,
  } from '@features/graphics-configurator';
  import { formatLabel } from '@entities/component';
  import { riskBadgeTone } from '@entities/operation';
  import { cn, fileNameFromPath } from '@shared/utils';
  import { Badge, BadgeGroup, Button, InfoTile, Select } from '@shared/ui';

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

  const noopArtifactSelection = () => undefined;
  const noopBuildPlan = () => undefined;

  const {
    row,
    selectedArtifact = '',
    riskLevel = null,
    busy = false,
    onArtifactSelection = noopArtifactSelection,
    onBuildPlan = noopBuildPlan,
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
</script>

<div class="grid gap-4 rounded-2xl border border-border-subtle bg-bg-elevated p-4">
  <header
    class={cn(
      'flex items-start justify-between gap-4 border-b border-border-subtle pb-3',
      'max-lg:flex-col max-lg:items-stretch',
    )}
  >
    <div class="grid min-w-0 gap-1">
      <strong class="text-base/tight text-text-strong">{view.fileName}</strong>
      <p class="text-sm/snug wrap-break-word text-text-muted" title={view.displayPath}>
        {view.displayPath}
      </p>
    </div>

    <BadgeGroup align="end" aria-label="Compatibility information">
      <Badge tone={riskBadgeTone(riskLevel)}>
        {formatLabel(row.component.swappability)}
      </Badge>

      {#if view.hasCandidates}
        <Badge>{view.compatibilityLabel}</Badge>
      {:else}
        <Badge tone="muted">No replacements</Badge>
      {/if}
    </BadgeGroup>
  </header>

  <div class={cn('grid grid-cols-2 gap-3', 'max-lg:grid-cols-1')}>
    <InfoTile label="Installed version" class="gap-2">
      <Select
        size="sm"
        disabled
        aria-label={view.installedSelectLabel}
        options={view.installedOptions}
        value={view.installedValue}
      />

      <small class="block text-xs/snug wrap-break-word text-text-subtle" title={view.currentPath}
        >{view.currentPath}</small
      >
    </InfoTile>

    <InfoTile label="Replacement version" class="gap-2">
      <Select
        size="sm"
        disabled={view.replacementSelectDisabled}
        aria-label={view.replacementSelectLabel}
        options={view.candidateOptions}
        value={view.replacementValue}
        onValueChange={handleArtifactSelection}
      />

      <small class="block text-xs/snug wrap-break-word text-text-subtle" title={view.candidatePath}
        >{view.candidatePath}</small
      >
    </InfoTile>
  </div>

  <footer
    class={cn(
      'flex items-start justify-between gap-4 border-t border-border-subtle pt-3',
      'max-lg:flex-col max-lg:items-stretch',
    )}
  >
    <div class="grid max-w-176 min-w-0 gap-1">
      <strong class="text-text-strong">{view.selectionSummaryTitle}</strong>
      <p class="text-sm/snug wrap-break-word text-text-muted">
        {view.candidateSummary}
      </p>
    </div>

    <Button
      variant="primary"
      size="sm"
      disabled={view.buildPlanDisabled}
      loading={busy}
      onclick={handleBuildPlan}
    >
      {busy ? 'Working...' : 'Build File Plan'}
    </Button>
  </footer>
</div>
