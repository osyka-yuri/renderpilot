<script lang="ts">
  import type { SwapPlan } from '@shared/api/types';
  import type { OperationHandler } from '@shared/utils/callbacks';
  import { formatLabel, formatRisk, riskTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';

  const BLOCKED_RISK_LEVEL = 'blocked';
  const UNKNOWN_VALUE = 'Unknown';
  const noopApply: OperationHandler = (_operationId: string): void => {};

  export let plan: SwapPlan | null = null;
  export let busy = false;
  export let onApply: OperationHandler = noopApply;

  $: canApply =
    !!plan && plan.blockers.length === 0 && plan.risk_level !== BLOCKED_RISK_LEVEL && !busy;

  function applyCurrentPlan(): void {
    if (!plan) {
      return;
    }

    onApply(plan.operation_id);
  }

  function displayValue(value?: string | null): string {
    return value ?? UNKNOWN_VALUE;
  }

  function planRiskBadgeTone(level: string): 'success' | 'warning' | 'danger' {
    const tone = riskTone(level);

    if (tone === 'low') {
      return 'success';
    }

    if (tone === 'medium') {
      return 'warning';
    }

    return 'danger';
  }

  function readinessCopy(currentPlan: SwapPlan): string {
    if (currentPlan.blockers.length > 0) {
      return 'Resolve blockers before applying this staged operation.';
    }

    if (currentPlan.warnings.length > 0) {
      return 'Review the warnings, then apply when you are satisfied with the staged replacement.';
    }

    return 'The staged replacement is ready to apply.';
  }
</script>

{#if plan}
  <section class="plan-card">
    <div class="plan-head">
      <div class="plan-copy">
        <p class="eyebrow">Operation Plan</p>
        <div class="plan-title-row">
          <div>
            <h3>{formatLabel(plan.operation_type)}</h3>
            <p class="plan-id">{plan.operation_id}</p>
          </div>
          <Badge pill size="md" tone={planRiskBadgeTone(plan.risk_level)}>
            Risk {formatRisk(plan.risk_level)}
          </Badge>
        </div>

        <div class="plan-flags">
          <Badge surface="outline" tone={plan.requires_backup ? 'warning' : 'muted'}>
            {plan.requires_backup ? 'Backup required' : 'Backup optional'}
          </Badge>
          <Badge surface="outline" tone={plan.requires_elevation ? 'warning' : 'muted'}>
            {plan.requires_elevation ? 'Elevation may be required' : 'No elevation expected'}
          </Badge>
          <Badge surface="outline" tone="muted">Confirmation attached</Badge>
        </div>
      </div>
    </div>

    <dl class="plan-grid">
      <div class="plan-metric">
        <dt>Target</dt>
        <dd>{plan.target_path}</dd>
      </div>
      <div class="plan-metric">
        <dt>Replacement</dt>
        <dd>{plan.replacement_path}</dd>
      </div>
      <div class="plan-metric">
        <dt>Current Version</dt>
        <dd>{displayValue(plan.original_version)}</dd>
      </div>
      <div class="plan-metric">
        <dt>New Version</dt>
        <dd>{displayValue(plan.replacement_version)}</dd>
      </div>
      <div class="plan-metric">
        <dt>Backup</dt>
        <dd>{plan.requires_backup ? 'Required before replacement' : 'Optional'}</dd>
      </div>
      <div class="plan-metric">
        <dt>Elevation</dt>
        <dd>{plan.requires_elevation ? 'May require elevation' : 'No elevation expected'}</dd>
      </div>
    </dl>

    {#if plan.warnings.length > 0 || plan.blockers.length > 0}
      <div class="plan-notes">
        {#if plan.warnings.length > 0}
          <section class="note-block warning">
            <div class="note-head">
              <strong>Warnings</strong>
              <Badge pill tone="warning">{plan.warnings.length}</Badge>
            </div>
            <ul class="note-list">
              {#each plan.warnings as warning}
                <li>{formatLabel(warning)}</li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if plan.blockers.length > 0}
          <section class="note-block blocked">
            <div class="note-head">
              <strong>Blockers</strong>
              <Badge pill tone="danger">{plan.blockers.length}</Badge>
            </div>
            <ul class="note-list">
              {#each plan.blockers as blocker}
                <li>{formatLabel(blocker)}</li>
              {/each}
            </ul>
          </section>
        {/if}
      </div>
    {/if}

    <div class="plan-actions">
      <div class="action-copy">
        <strong>Ready to apply</strong>
        <p>{readinessCopy(plan)}</p>
      </div>
      <Button
        variant="primary"
        size="sm"
        disabled={!canApply}
        loading={busy}
        onclick={applyCurrentPlan}
      >
        {busy ? 'Applying...' : 'Apply Operation'}
      </Button>
    </div>
  </section>
{/if}

<style>
  .plan-card {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    border-radius: var(--radius-xl);
    background: var(--bg-card);
    border: 1px solid var(--border-subtle);
    box-shadow: var(--shadow-card);
  }

  .plan-head {
    display: grid;
    gap: var(--space-3);
  }

  .plan-copy {
    display: grid;
    gap: var(--space-2);
  }

  .plan-title-row {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: flex-start;
    flex-wrap: wrap;
  }

  .eyebrow {
    margin: 0 0 var(--space-1);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.6875rem;
    color: var(--text-subtle);
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
  }

  .plan-id {
    margin: var(--space-1) 0 0;
    color: var(--text-muted);
    word-break: break-word;
  }

  .plan-flags {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .plan-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-2);
  }

  .plan-metric {
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-subtle);
    background: var(--bg-soft);
  }

  dt {
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    margin-bottom: var(--space-1);
  }

  dd {
    margin: 0;
    word-break: break-word;
    color: var(--text-strong);
  }

  .plan-notes {
    display: grid;
    gap: var(--space-2);
  }

  .note-block {
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-subtle);
    background: var(--bg-soft);
  }

  .note-head {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: center;
  }

  .note-list {
    display: grid;
    gap: var(--space-2);
    margin: var(--space-2) 0 0;
    padding: 0;
    list-style: none;
  }

  .note-list li {
    padding-left: var(--space-4);
    position: relative;
    color: var(--text-soft);
    line-height: 1.45;
  }

  .note-list li::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0.6rem;
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.55;
  }

  .warning {
    background: color-mix(in srgb, var(--warning) 6%, var(--bg-control));
    border-color: color-mix(in srgb, var(--warning) 18%, var(--border-subtle));
  }

  .blocked {
    background: color-mix(in srgb, var(--danger) 7%, var(--bg-control));
    border-color: color-mix(in srgb, var(--danger) 18%, var(--border-subtle));
  }

  .plan-actions {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: center;
    flex-wrap: wrap;
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .action-copy {
    display: grid;
    gap: 0.18rem;
  }

  .plan-actions p {
    margin: 0;
    color: var(--text-muted);
  }

  @media (max-width: 720px) {
    .plan-actions :global(button) {
      width: 100%;
      justify-content: center;
    }
  }
</style>
