<script lang="ts">
  import type { SwapPlan } from '@shared/api/types';
  import type { OperationHandler } from '@shared/utils/callbacks';
  import { cx } from '@shared/utils/cx';
  import { formatLabel, formatRisk, riskTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';

  type RiskBadgeTone = 'success' | 'warning' | 'danger';

  type PlanMetric = {
    label: string;
    value: string;
  };

  type PlanFlag = {
    label: string;
    tone: 'warning' | 'muted';
  };

  type PlanNoteGroup = {
    id: 'warnings' | 'blockers';
    title: string;
    tone: 'warning' | 'danger';
    className: 'warning' | 'blocked';
    items: string[];
  };

  const BLOCKED_RISK_LEVEL = 'blocked';
  const UNKNOWN_VALUE = 'Unknown';

  export let plan: SwapPlan | null = null;
  export let busy = false;
  export let onApply: OperationHandler = () => {
    return;
  };

  $: canApply = plan ? canApplyPlan(plan, busy) : false;

  $: planTitle = plan ? formatLabel(plan.operation_type) : '';
  $: planRiskLabel = plan ? formatRisk(plan.risk_level) : UNKNOWN_VALUE;
  $: planRiskTone = plan ? getPlanRiskBadgeTone(plan.risk_level) : 'danger';

  $: planFlags = plan ? getPlanFlags(plan) : [];
  $: planMetrics = plan ? getPlanMetrics(plan) : [];
  $: planNoteGroups = plan ? getPlanNoteGroups(plan) : [];

  $: hasPlanNotes = planNoteGroups.length > 0;
  $: readinessText = plan ? getReadinessCopy(plan) : '';

  function canApplyPlan(currentPlan: SwapPlan, isBusy: boolean): boolean {
    return (
      !isBusy && currentPlan.blockers.length === 0 && currentPlan.risk_level !== BLOCKED_RISK_LEVEL
    );
  }

  function applyCurrentPlan(): void {
    if (!plan || !canApply) {
      return;
    }

    onApply(plan.operation_id);
  }

  function displayValue(value?: string | null): string {
    const normalizedValue = value?.trim();

    return normalizedValue ?? UNKNOWN_VALUE;
  }

  function getPlanRiskBadgeTone(level: string): RiskBadgeTone {
    switch (riskTone(level)) {
      case 'low':
        return 'success';

      case 'medium':
        return 'warning';

      default:
        return 'danger';
    }
  }

  function getPlanFlags(currentPlan: SwapPlan): PlanFlag[] {
    return [
      {
        label: currentPlan.requires_backup ? 'Backup required' : 'Backup optional',
        tone: currentPlan.requires_backup ? 'warning' : 'muted',
      },
      {
        label: currentPlan.requires_elevation
          ? 'Elevation may be required'
          : 'No elevation expected',
        tone: currentPlan.requires_elevation ? 'warning' : 'muted',
      },
      {
        label: 'Confirmation attached',
        tone: 'muted',
      },
    ];
  }

  function getPlanMetrics(currentPlan: SwapPlan): PlanMetric[] {
    return [
      {
        label: 'Target',
        value: displayValue(currentPlan.target_path),
      },
      {
        label: 'Replacement',
        value: displayValue(currentPlan.replacement_path),
      },
      {
        label: 'Current version',
        value: displayValue(currentPlan.original_version),
      },
      {
        label: 'New version',
        value: displayValue(currentPlan.replacement_version),
      },
      {
        label: 'Backup',
        value: currentPlan.requires_backup ? 'Required before replacement' : 'Optional',
      },
      {
        label: 'Elevation',
        value: currentPlan.requires_elevation ? 'May require elevation' : 'No elevation expected',
      },
    ];
  }

  function getPlanNoteGroups(currentPlan: SwapPlan): PlanNoteGroup[] {
    const noteGroups: PlanNoteGroup[] = [];

    if (currentPlan.warnings.length > 0) {
      noteGroups.push({
        id: 'warnings',
        title: 'Warnings',
        tone: 'warning',
        className: 'warning',
        items: currentPlan.warnings.map(formatLabel),
      });
    }

    if (currentPlan.blockers.length > 0) {
      noteGroups.push({
        id: 'blockers',
        title: 'Blockers',
        tone: 'danger',
        className: 'blocked',
        items: currentPlan.blockers.map(formatLabel),
      });
    }

    return noteGroups;
  }

  function getReadinessCopy(currentPlan: SwapPlan): string {
    if (currentPlan.blockers.length > 0) {
      return 'Resolve blockers before applying this staged operation.';
    }

    if (currentPlan.risk_level === BLOCKED_RISK_LEVEL) {
      return 'This staged operation is blocked by its risk level.';
    }

    if (currentPlan.warnings.length > 0) {
      return 'Review the warnings, then apply when you are satisfied with the staged replacement.';
    }

    return 'The staged replacement is ready to apply.';
  }
</script>

{#if plan}
  <section class="plan-card" aria-labelledby="operation-plan-title">
    <header class="plan-head">
      <div class="plan-copy">
        <p class="eyebrow">Operation Plan</p>

        <div class="plan-title-row">
          <div>
            <h3 id="operation-plan-title">{planTitle}</h3>
            <p class="plan-id">{plan.operation_id}</p>
          </div>

          <Badge pill size="md" tone={planRiskTone}>
            Risk {planRiskLabel}
          </Badge>
        </div>

        <div class="plan-flags" aria-label="Operation requirements">
          {#each planFlags as flag (flag.label)}
            <Badge surface="outline" tone={flag.tone}>{flag.label}</Badge>
          {/each}
        </div>
      </div>
    </header>

    <dl class="plan-grid">
      {#each planMetrics as metric (metric.label)}
        <div class="plan-metric">
          <dt>{metric.label}</dt>
          <dd>{metric.value}</dd>
        </div>
      {/each}
    </dl>

    {#if hasPlanNotes}
      <div class="plan-notes">
        {#each planNoteGroups as noteGroup (noteGroup.id)}
          <section class={cx('note-block', noteGroup.className)}>
            <div class="note-head">
              <strong>{noteGroup.title}</strong>
              <Badge pill tone={noteGroup.tone}>{noteGroup.items.length}</Badge>
            </div>

            <ul class="note-list">
              {#each noteGroup.items as item, index (`${noteGroup.id}-${index}`)}
                <li>{item}</li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}

    <footer class="plan-actions">
      <div class="action-copy">
        <strong>{canApply ? 'Ready to apply' : 'Not ready to apply'}</strong>
        <p>{readinessText}</p>
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
    </footer>
  </section>
{/if}

<style>
  .plan-card {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
    background: var(--bg-card);
    box-shadow: var(--shadow-card);
  }

  .plan-head,
  .plan-copy,
  .plan-notes,
  .note-list,
  .action-copy {
    display: grid;
  }

  .plan-head,
  .plan-copy,
  .plan-notes {
    gap: var(--space-3);
  }

  .plan-copy,
  .note-list {
    gap: var(--space-2);
  }

  .action-copy {
    gap: 0.18rem;
  }

  .plan-title-row,
  .plan-flags,
  .note-head,
  .plan-actions {
    display: flex;
  }

  .plan-title-row {
    justify-content: space-between;
    align-items: flex-start;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .plan-flags {
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .eyebrow,
  h3,
  .plan-id,
  .plan-actions p {
    margin: 0;
  }

  .eyebrow {
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  h3 {
    color: var(--text-strong);
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .plan-id {
    margin-top: var(--space-1);
    color: var(--text-muted);
    word-break: break-word;
  }

  .plan-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-2);
    margin: 0;
  }

  .plan-metric,
  .note-block {
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-soft);
  }

  dt {
    margin-bottom: var(--space-1);
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  dd {
    margin: 0;
    color: var(--text-strong);
    word-break: break-word;
  }

  .note-head {
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
  }

  .note-head strong,
  .action-copy strong {
    color: var(--text-strong);
  }

  .note-list {
    margin: var(--space-2) 0 0;
    padding: 0;
    list-style: none;
  }

  .note-list li {
    position: relative;
    padding-left: var(--space-4);
    color: var(--text-soft);
    line-height: 1.45;
  }

  .note-list li::before {
    content: '';
    position: absolute;
    top: 0.6rem;
    left: 0;
    width: 0.35rem;
    height: 0.35rem;
    border-radius: 999px;
    background: currentColor;
    opacity: 0.55;
  }

  .warning {
    border-color: color-mix(in srgb, var(--warning) 18%, var(--border-subtle));
    background: color-mix(in srgb, var(--warning) 6%, var(--bg-control));
  }

  .blocked {
    border-color: color-mix(in srgb, var(--danger) 18%, var(--border-subtle));
    background: color-mix(in srgb, var(--danger) 7%, var(--bg-control));
  }

  .plan-actions {
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .plan-actions p {
    color: var(--text-muted);
  }

  @media (max-width: 720px) {
    .plan-actions :global(button) {
      width: 100%;
      justify-content: center;
    }
  }
</style>
