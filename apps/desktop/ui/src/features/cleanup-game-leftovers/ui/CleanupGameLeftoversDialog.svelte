<script lang="ts">
  import Trash2Icon from '@lucide/svelte/icons/trash-2';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
    Alert,
    AlertDescription,
    AlertTitle,
    Badge,
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Item,
    ItemContent,
    ItemDescription,
    ItemTitle,
    ScrollArea,
    Spinner,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { CleanupFlow } from '../model/cleanup-flow.svelte';
  import { categoryLabel, issueLabel, statusLabel, stepLabel } from '../model/labels';

  const { flow }: { flow: CleanupFlow } = $props();
  const dialogId = $props.id();
  const canClean = $derived(flow.proposals.some((proposal) => proposal.canClean));

  function getOpen(): boolean {
    return flow.proposals.length > 0;
  }

  function setOpen(open: boolean): void {
    if (!open && !flow.busy) {
      void flow.leaveAll();
    }
  }
</script>

<Dialog bind:open={getOpen, setOpen}>
  <DialogContent
    closeLabel={t('leftovers.leave')}
    showCloseButton={!flow.busy}
    escapeKeydownBehavior={flow.busy ? 'ignore' : 'close'}
    interactOutsideBehavior={flow.busy ? 'ignore' : 'close'}
    class="max-h-[calc(100dvh-2rem)] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden"
  >
    <DialogHeader>
      <DialogTitle>{t('leftovers.title')}</DialogTitle>
      <DialogDescription>{t('leftovers.description')}</DialogDescription>
    </DialogHeader>

    <ScrollArea
      type="auto"
      class="min-h-0 rounded-md"
      viewportRegion={{ label: t('leftovers.title') }}
      viewportFocusable
    >
      <div class="grid gap-4 p-1 text-sm" aria-busy={flow.busy}>
        <Accordion type="multiple" class="min-w-0">
          {#each flow.proposals as proposal, index (proposal.gameId)}
            {@const contentId = `${dialogId}-game-${index}`}
            {@const outcome = flow.outcomes[proposal.gameId]}
            <AccordionItem value={proposal.gameId}>
              <AccordionTrigger
                aria-controls={contentId}
                class="items-center py-3 hover:no-underline"
              >
                <span class="grid min-w-0 flex-1 gap-1">
                  <span class="flex min-w-0 items-center gap-2">
                    <span class="truncate" title={proposal.gameName}>{proposal.gameName}</span>
                    <Badge variant="secondary">{proposal.items.length}</Badge>
                    {#if flow.activeGameId === proposal.gameId}<Spinner />{/if}
                  </span>
                  <span
                    class="truncate text-xs font-normal text-muted-foreground"
                    title={proposal.installPath}>{proposal.installPath}</span
                  >
                </span>
              </AccordionTrigger>

              {#if flow.errors[proposal.gameId]}
                <Alert variant="destructive" size="sm" role="alert" class="mb-3">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertDescription>{flow.errors[proposal.gameId].message}</AlertDescription>
                </Alert>
              {/if}

              {#if flow.leaveIssues[proposal.gameId]}
                <Alert variant="warning" size="sm" role="alert" class="mb-3">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertDescription
                    >{issueLabel(flow.leaveIssues[proposal.gameId].code)}</AlertDescription
                  >
                </Alert>
              {/if}

              {#if outcome}
                <Alert variant="warning" size="sm" role="status" class="mb-3">
                  <TriangleAlertIcon aria-hidden="true" />
                  <AlertTitle>{statusLabel(outcome.status)}</AlertTitle>
                  {#if outcome.issue !== null}
                    <AlertDescription>{issueLabel(outcome.issue.code)}</AlertDescription>
                  {/if}
                </Alert>
              {/if}

              <AccordionContent id={contentId}>
                <div class="grid min-w-0 gap-3">
                  {#if outcome}
                    <ul class="grid gap-1 text-xs text-muted-foreground">
                      {#each outcome.steps as step (step.itemId)}
                        <li>{categoryLabel(step.category)}: {stepLabel(step.outcome)}</li>
                      {/each}
                    </ul>
                  {/if}
                  <div class="grid gap-2">
                    {#each proposal.items as item (item.itemId)}
                      <Item variant="muted" size="sm">
                        <ItemContent>
                          <ItemTitle>{categoryLabel(item.category)}</ItemTitle>
                          {#if item.path !== null}
                            <ItemDescription class="line-clamp-none text-start text-wrap break-all">
                              {item.path}
                            </ItemDescription>
                          {/if}
                          {#if item.issue !== null}
                            <ItemDescription class="line-clamp-none text-start text-wrap">
                              {issueLabel(item.issue.code)}
                            </ItemDescription>
                          {:else if item.disposition === 'blocked'}
                            <ItemDescription>{t('leftovers.outcome.blocked')}</ItemDescription>
                          {/if}
                        </ItemContent>
                      </Item>
                    {/each}
                  </div>
                </div>
              </AccordionContent>
            </AccordionItem>
          {/each}
        </Accordion>

        {#each flow.issues as entry, index (`${entry.gameId}:${entry.issue.code}:${index}`)}
          <Alert variant="warning" size="sm" role="note">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertDescription>{issueLabel(entry.issue.code)}</AlertDescription>
          </Alert>
        {/each}
        <p class="text-xs text-muted-foreground">{t('leftovers.leaveHint')}</p>
      </div>
    </ScrollArea>

    <DialogFooter>
      <Button variant="outline" disabled={flow.busy} onclick={() => void flow.leaveAll()}>
        {#if flow.action === 'leave'}<Spinner />{/if}
        {t('leftovers.leave')}
      </Button>
      <Button
        variant="destructive"
        disabled={flow.busy || !canClean}
        onclick={() => void flow.cleanAll()}
      >
        {#if flow.action === 'clean'}
          <Spinner />
        {:else}
          <Trash2Icon class="size-4" aria-hidden="true" />
        {/if}
        {t(flow.action === 'clean' ? 'leftovers.cleaning' : 'leftovers.clean')}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
