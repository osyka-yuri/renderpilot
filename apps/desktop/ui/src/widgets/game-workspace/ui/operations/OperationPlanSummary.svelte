<script lang="ts">
  import { cn } from '@shared/classnames';
  import {
    formatRisk,
    riskBadgeVariant,
    type OperationBadgeVariant,
    type OperationHandler,
    type SwapPlan,
  } from '@entities/operation';
  import { formatLabel } from '@entities/component';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    Badge,
    Button,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
  } from '@shared/ui';

  type PlanMetric = {
    label: string;
    value: string;
  };

  type PlanFlag = {
    label: string;
    variant: OperationBadgeVariant;
  };

  type PlanNoteGroup = {
    id: 'warnings' | 'blockers';
    title: string;
    variant: OperationBadgeVariant;
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
  const planRiskVariant = $derived.by(
    (): OperationBadgeVariant => (plan ? riskBadgeVariant(plan.risk_level) : 'destructive'),
  );

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

  function getPlanFlags(currentPlan: SwapPlan): PlanFlag[] {
    return [
      {
        label: currentPlan.requires_backup ? 'Backup required' : 'Backup optional',
        variant: currentPlan.requires_backup ? 'secondary' : 'outline',
      },
      {
        label: currentPlan.requires_elevation
          ? 'Elevation may be required'
          : 'No elevation expected',
        variant: currentPlan.requires_elevation ? 'secondary' : 'outline',
      },
      {
        label: 'Confirmation attached',
        variant: 'outline',
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
        variant: 'secondary',
        className: 'warning',
        items: currentPlan.warnings.map(formatLabel),
      });
    }

    if (currentPlan.blockers.length > 0) {
      noteGroups.push({
        id: 'blockers',
        title: 'Blockers',
        variant: 'destructive',
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
  <section aria-labelledby="operation-plan-title">
    <Card>
      <CardHeader>
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="grid gap-1">
            <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
              Operation Plan
            </p>
            <CardTitle id="operation-plan-title">{planTitle}</CardTitle>
            <CardDescription>{plan.operation_id}</CardDescription>
          </div>

          <Badge variant={planRiskVariant}>
            Risk {planRiskLabel}
          </Badge>
        </div>

        <div class="flex flex-wrap gap-2" aria-label="Operation requirements">
          {#each planFlags as flag (flag.label)}
            <Badge variant={flag.variant}>
              {flag.label}
            </Badge>
          {/each}
        </div>
      </CardHeader>

      <CardContent>
        <dl class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-x-4 gap-y-3">
          {#each planMetrics as metric (metric.label)}
            <div class="grid min-w-0 gap-1">
              <dt class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                {metric.label}
              </dt>
              <dd class="text-sm/5 font-semibold text-foreground">{metric.value}</dd>
            </div>
          {/each}
        </dl>

        {#if hasPlanNotes}
          <div class="grid gap-3">
            {#each planNoteGroups as noteGroup (noteGroup.id)}
              <Alert variant={noteGroup.className === 'blocked' ? 'destructive' : 'default'}>
                <div class="flex items-center justify-between gap-3">
                  <AlertTitle>{noteGroup.title}</AlertTitle>
                  <Badge variant={noteGroup.variant}>
                    {noteGroup.items.length}
                  </Badge>
                </div>

                <AlertDescription>
                  <ul class="mt-2 grid list-disc gap-1.5 pl-4">
                    {#each noteGroup.items as item, index (`${noteGroup.id}-${index}`)}
                      <li class="leading-snug">{item}</li>
                    {/each}
                  </ul>
                </AlertDescription>
              </Alert>
            {/each}
          </div>
        {/if}

        <footer
          class={cn(
            'flex flex-wrap items-center justify-between gap-3',
            'max-md:flex-col max-md:items-stretch',
          )}
        >
          <div class="grid gap-0.5">
            <strong class="text-foreground"
              >{canApply ? 'Ready to apply' : 'Not ready to apply'}</strong
            >
            <p class="text-muted-foreground">{readinessText}</p>
          </div>

          <Button variant="default" size="sm" disabled={!canApply} onclick={applyCurrentPlan}>
            {busy ? 'Applying...' : 'Apply Operation'}
          </Button>
        </footer>
      </CardContent>
    </Card>
  </section>
{/if}
