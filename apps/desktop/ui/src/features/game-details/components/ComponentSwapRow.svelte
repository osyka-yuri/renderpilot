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
  export let onArtifactSelection: (componentId: string, value: string) => void;
  export let onBuildPlan: (componentId: string, artifactId: string) => void;
</script>

<div class="technology-row">
  <div class="file-meta">
    <div>
      <strong>{fileNameFromPath(row.currentInstalled.path)}</strong>
      <p>{row.group?.file_path ?? row.currentInstalled.path}</p>
    </div>

    <div class="library-badges">
      <Badge tone={riskBadgeTone(riskLevel)}>
        {formatLabel(row.component.swappability)}
      </Badge>
      {#if row.group?.candidates.length}
        <Badge>{row.group.candidates.length} compatible {row.group.candidates.length === 1 ? 'version' : 'versions'}</Badge>
      {:else}
        <Badge tone="muted">No replacements</Badge>
      {/if}
    </div>
  </div>

  <div class="config-grid">
    <label class="config-field">
      <span class="field-label">Installed version</span>
      <Select
        size="sm"
        disabled
        ariaLabel="Installed version"
        options={installedOptionsForRow(row)}
        value={row.installedValue}
      />
      <small>{row.currentInstalled.path}</small>
    </label>

    <label class="config-field">
      <span class="field-label">Replacement version</span>
      <Select
        size="sm"
        disabled={!row.group || row.group.candidates.length === 0 || busy}
        ariaLabel="Replacement version"
        options={candidateOptionsForRow(row)}
        value={selectedArtifact}
        onValueChange={(value) => onArtifactSelection(row.component.id, value)}
      />
      <small>{row.candidatePath}</small>
    </label>
  </div>

  <div class="action-row">
    <div class="selection-summary">
      <strong>{row.selectedCandidate ? 'Selected replacement' : 'Replacement selection'}</strong>
      <p>{row.candidateSummary}</p>
    </div>

    <Button
      variant="primary"
      size="sm"
      disabled={!row.canBuildPlan}
      loading={busy}
      onclick={() => row.selectedCandidate && onBuildPlan(row.component.id, row.selectedCandidate.artifact_id)}
    >
      {busy ? 'Working...' : 'Build File Plan'}
    </Button>
  </div>
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
    gap: 1rem;
    align-items: flex-start;
  }

  .file-meta {
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--border-subtle);
  }

  .file-meta strong,
  .selection-summary strong {
    color: var(--text-strong);
  }

  .file-meta strong {
    font-size: 0.98rem;
    line-height: 1.25;
  }

  .file-meta p,
  .selection-summary p {
    margin: var(--space-1) 0 0;
    color: var(--text-muted);
    font-size: 0.84rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .library-badges {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-top: var(--space-2);
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
    gap: var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .selection-summary {
    display: grid;
    gap: var(--space-1);
    max-width: 44rem;
  }

  .selection-summary p {
    margin: 0;
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
