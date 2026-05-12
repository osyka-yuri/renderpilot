<script lang="ts">
  import { cn } from '@shared/utils';
  import { formatRisk, riskTone, type OperationHandler, type SwapPlan } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import { Badge, Button, DefinitionMetric, SectionHeader, Surface } from '@shared/ui';

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

  type Props = {
    plan?: SwapPlan | null;
    busy?: boolean;
    onApply?: OperationHandler;
  };

  const {
    plan = null,
    busy = false,
    onApply = () => {
      return;
    },
  }: Props = $props();

  const canApply = $derived(plan ? canApplyPlan(plan, busy) : false);

  const planTitle = $derived(plan ? formatLabel(plan.operation_type) : '');
  const planRiskLabel = $derived(plan ? formatRisk(plan.risk_level) : UNKNOWN_VALUE);
  const planRiskTone = $derived(plan ? getPlanRiskBadgeTone(plan.risk_level) : 'danger');

  const planFlags = $derived(plan ? getPlanFlags(plan) : []);
  const planMetrics = $derived(plan ? getPlanMetrics(plan) : []);
  const planNoteGroups = $derived(plan ? getPlanNoteGroups(plan) : []);

  const hasPlanNotes = $derived(planNoteGroups.length > 0);
  const readinessText = $derived(plan ? getReadinessCopy(plan) : '');

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
  <Surface as="section" shadow class="grid gap-3 p-4" aria-labelledby="operation-plan-title">
    <header class="grid gap-3 border-b border-border-subtle pb-3">
      <SectionHeader
        eyebrow="Operation Plan"
        title={planTitle}
        titleId="operation-plan-title"
        description={plan.operation_id}
        class="px-0"
      >
        <Badge pill size="md" tone={planRiskTone}>
          Risk {planRiskLabel}
        </Badge>
      </SectionHeader>

      <div class="flex flex-wrap gap-2" aria-label="Operation requirements">
        {#each planFlags as flag (flag.label)}
          <Badge surface="outline" tone={flag.tone}>{flag.label}</Badge>
        {/each}
      </div>
    </header>

    <dl class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-2">
      {#each planMetrics as metric (metric.label)}
        <DefinitionMetric label={metric.label}>{metric.value}</DefinitionMetric>
      {/each}
    </dl>

    {#if hasPlanNotes}
      <div class="grid gap-3">
        {#each planNoteGroups as noteGroup (noteGroup.id)}
          <section
            class={cn(
              'rounded-2xl border border-border-subtle p-3',
              noteGroup.className === 'warning'
                ? 'border-warning/20 bg-warning/10'
                : 'border-danger/20 bg-danger/10',
            )}
          >
            <div class="flex items-center justify-between gap-3">
              <strong class="text-text-strong">{noteGroup.title}</strong>
              <Badge pill tone={noteGroup.tone}>{noteGroup.items.length}</Badge>
            </div>

            <ul class="mt-2 grid list-none gap-2 p-0">
              {#each noteGroup.items as item, index (`${noteGroup.id}-${index}`)}
                <li
                  class={cn(
                    'relative pl-4 leading-snug text-text-soft',
                    'before:absolute before:top-2.5 before:left-0 before:size-1.5',
                    'before:rounded-full before:bg-current before:opacity-55',
                  )}
                >
                  {item}
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}

    <footer
      class={cn(
        'flex flex-wrap items-center justify-between gap-3 border-t',
        'border-border-subtle pt-3',
        'max-md:flex-col max-md:items-stretch',
      )}
    >
      <div class="grid gap-0.5">
        <strong class="text-text-strong"
          >{canApply ? 'Ready to apply' : 'Not ready to apply'}</strong
        >
        <p class="text-text-muted">{readinessText}</p>
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
  </Surface>
{/if}
