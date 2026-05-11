<script lang="ts">
  import {
    candidateOptionsForRow,
    installedOptionsForRow,
    type ConfiguredComponentRow,
  } from '@features/graphics-configurator';
  import { formatLabel } from '@entities/component';
  import { riskBadgeTone } from '@entities/operation';
  import { fileNameFromPath } from '@shared/utils';
  import { Badge, BadgeGroup, Button, Select } from '@shared/ui';

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

  let {
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

<div class="library-row">
  <header class="file-meta">
    <div class="file-info">
      <strong>{view.fileName}</strong>
      <p title={view.displayPath}>{view.displayPath}</p>
    </div>

    <BadgeGroup class="library-badges" align="end" aria-label="Compatibility information">
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

  <div class="config-grid">
    <div class="config-field">
      <span class="field-label">Installed version</span>

      <Select
        size="sm"
        disabled
        aria-label={view.installedSelectLabel}
        options={view.installedOptions}
        value={view.installedValue}
      />

      <small title={view.currentPath}>{view.currentPath}</small>
    </div>

    <div class="config-field">
      <span class="field-label">Replacement version</span>

      <Select
        size="sm"
        disabled={view.replacementSelectDisabled}
        aria-label={view.replacementSelectLabel}
        options={view.candidateOptions}
        value={view.replacementValue}
        onValueChange={handleArtifactSelection}
      />

      <small title={view.candidatePath}>{view.candidatePath}</small>
    </div>
  </div>

  <footer class="action-row">
    <div class="selection-summary" aria-live="polite">
      <strong>{view.selectionSummaryTitle}</strong>
      <p>{view.candidateSummary}</p>
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

<style>
  .library-row {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
  }

  .file-meta,
  .action-row {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    align-items: flex-start;
  }

  .file-meta {
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--border-subtle);
  }

  .file-info,
  .selection-summary {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .file-info strong,
  .selection-summary strong {
    color: var(--text-strong);
  }

  .file-info strong {
    font-size: 0.98rem;
    line-height: 1.25;
  }

  .file-info p,
  .selection-summary p {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.84rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .config-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .config-field {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-soft);
  }

  .field-label {
    display: block;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  small {
    display: block;
    color: var(--text-subtle);
    font-size: 0.78rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .action-row {
    align-items: center;
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .selection-summary {
    max-width: 44rem;
  }

  @media (max-width: 820px) {
    .config-grid {
      grid-template-columns: 1fr;
    }

    .file-meta,
    .action-row {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
