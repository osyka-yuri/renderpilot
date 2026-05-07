<script lang="ts">
  import {
    candidateOptionsForRow,
    fileNameFromPath,
    installedOptionsForRow,
    type ConfiguredComponentRow,
  } from '@features/game-details/lib/graphics-configurator';
  import { formatLabel, riskBadgeTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Select from '@shared/ui/Select.svelte';

  export let row: ConfiguredComponentRow;
  export let selectedArtifact = '';
  export let riskLevel: string | null | undefined = null;
  export let busy = false;

  export let onArtifactSelection: (componentId: string, value: string) => void = () => {
    return;
  };
  export let onBuildPlan: (componentId: string, artifactId: string) => void = () => {
    return;
  };

  $: componentId = row.component.id;
  $: currentPath = row.currentInstalled.path;
  $: displayPath = row.group?.file_path ?? currentPath;
  $: fileName = fileNameFromPath(currentPath);

  $: installedOptions = installedOptionsForRow(row);
  $: candidateOptions = candidateOptionsForRow(row);

  $: candidatesCount = row.group?.candidates.length ?? 0;
  $: hasCandidates = candidatesCount > 0;

  $: replacementSelectDisabled = !hasCandidates || busy;
  $: selectedCandidate = row.selectedCandidate;
  $: selectedArtifactId = selectedCandidate?.artifact_id;

  $: buildPlanDisabled = busy || !row.canBuildPlan || !selectedArtifactId;

  $: compatibilityLabel =
    candidatesCount === 1 ? '1 compatible version' : `${candidatesCount} compatible versions`;

  function handleArtifactSelection(value: string) {
    onArtifactSelection(componentId, value);
  }

  function handleBuildPlan() {
    if (!selectedArtifactId || buildPlanDisabled) {
      return;
    }

    onBuildPlan(componentId, selectedArtifactId);
  }
</script>

<div class="technology-row">
  <header class="file-meta">
    <div class="file-info">
      <strong>{fileName}</strong>
      <p>{displayPath}</p>
    </div>

    <div class="library-badges" aria-label="Compatibility information">
      <Badge tone={riskBadgeTone(riskLevel)}>
        {formatLabel(row.component.swappability)}
      </Badge>

      {#if hasCandidates}
        <Badge>{compatibilityLabel}</Badge>
      {:else}
        <Badge tone="muted">No replacements</Badge>
      {/if}
    </div>
  </header>

  <div class="config-grid">
    <label class="config-field">
      <span class="field-label">Installed version</span>

      <Select
        size="sm"
        disabled
        ariaLabel={`Installed version for ${fileName}`}
        options={installedOptions}
        value={row.installedValue}
      />

      <small>{currentPath}</small>
    </label>

    <label class="config-field">
      <span class="field-label">Replacement version</span>

      <Select
        size="sm"
        disabled={replacementSelectDisabled}
        ariaLabel={`Replacement version for ${fileName}`}
        options={candidateOptions}
        value={selectedArtifact}
        onValueChange={handleArtifactSelection}
      />

      <small>{row.candidatePath}</small>
    </label>
  </div>

  <footer class="action-row">
    <div class="selection-summary">
      <strong>{selectedCandidate ? 'Selected replacement' : 'Replacement selection'}</strong>
      <p>{row.candidateSummary}</p>
    </div>

    <Button
      variant="primary"
      size="sm"
      disabled={buildPlanDisabled}
      loading={busy}
      onclick={handleBuildPlan}
    >
      {busy ? 'Working...' : 'Build File Plan'}
    </Button>
  </footer>
</div>

<style>
  .technology-row {
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

  .library-badges {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    justify-content: flex-end;
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
    }

    .library-badges {
      justify-content: flex-start;
    }

    .action-row :global(button) {
      width: 100%;
      justify-self: stretch;
    }
  }
</style>
