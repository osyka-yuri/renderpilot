<script lang="ts">
  import InfoIcon from '@lucide/svelte/icons/info';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
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
    Label,
    RadioGroup,
    RadioGroupItem,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    Spinner,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import {
    decisionOptions,
    hasRootCorrection,
    type AddGameConfirmation,
    type AddGameRootChoice,
    type RootCorrectionBlockerKind,
  } from '../model/add-game';
  import type { AddGameDialogState } from '../model/add-game-flow.svelte';
  import {
    addGameUnavailableReasonCoversWarning,
    formatAddGameUnavailableReason,
  } from '../model/add-game-unavailable';
  import { formatAddGameWarning } from '../model/add-game-warning';

  type Props = {
    state: AddGameDialogState;
    onClose: () => void;
    onChooseFolder: () => void | Promise<void>;
    onConfirm: (confirmation: AddGameConfirmation) => void | Promise<void>;
    onRollbackAndConfirm: (confirmation: AddGameConfirmation) => void | Promise<void>;
  };

  let {
    state: dialogState,
    onClose,
    onChooseFolder,
    onConfirm,
    onRollbackAndConfirm,
  }: Props = $props();
  let chosenExecutable = $state('');
  let rootChoice = $state<AddGameRootChoice>('selected');
  let inspectionIdentity = '';

  const inspection = $derived(dialogState.inspection);
  const busy = $derived(dialogState.kind !== 'review');
  const rootCorrection = $derived(inspection.rootCorrection);
  const validExecutables = $derived(
    inspection.executables.filter((candidate) => candidate.validWindowsPe),
  );
  const unavailableReasons = $derived(
    inspection.decision.kind === 'unavailable' ? inspection.decision.reasons : [],
  );
  const visibleWarnings = $derived(
    inspection.warnings.filter(
      (warning) =>
        (warning.code !== 'inside_existing_install' || !hasRootCorrection(inspection)) &&
        !unavailableReasons.some((reason) =>
          addGameUnavailableReasonCoversWarning(reason, warning.code),
        ),
    ),
  );
  const standaloneUnavailableReasons = $derived(
    unavailableReasons.filter(
      (reason) => reason !== 'root_correction_blocked' || rootCorrection?.status !== 'blocked',
    ),
  );
  const chosenExecutableLabel = $derived(
    validExecutables.find((candidate) => candidate.path === chosenExecutable)?.relativePath ??
      t('addGame.chooseExecutablePlaceholder'),
  );
  const recommendedRoot = $derived(inspection.recommendation?.root ?? null);
  const options = $derived(decisionOptions(inspection.decision));
  const selectedOption = $derived(options.find((option) => option.rootChoice === rootChoice));
  const selectedRootSelectable = $derived(
    options.some((option) => option.rootChoice === 'selected'),
  );
  const hasRecommendedAlternative = $derived(
    recommendedRoot !== null &&
      recommendedRoot !== inspection.selectedRoot &&
      options.some((option) => option.rootChoice === 'recommended'),
  );
  const requiresExecutableChoice = $derived(
    inspection.requiresExplicitExecutable && chosenExecutable.length === 0,
  );
  const normalConfirmationBlocked = $derived(
    requiresExecutableChoice ||
      selectedOption === undefined ||
      selectedOption.catalogAction === 'correct_existing_root',
  );

  $effect(() => {
    const nextIdentity = `${inspection.inspectionFingerprint}\u0000${inspection.decision.kind}`;
    if (nextIdentity !== inspectionIdentity) {
      inspectionIdentity = nextIdentity;
      chosenExecutable = '';
      rootChoice =
        inspection.decision.kind === 'review'
          ? inspection.decision.defaultOption.rootChoice
          : inspection.decision.kind === 'automatic'
            ? inspection.decision.option.rootChoice
            : 'selected';
    }
  });

  function confirmSelectedRootCorrection(): void {
    const confirmation = rootCorrectionConfirmation();
    if (confirmation !== null) {
      void onConfirm(confirmation);
    }
  }

  function rollbackAndConfirmRootCorrection(): void {
    const confirmation = rootCorrectionConfirmation();
    if (confirmation !== null) {
      void onRollbackAndConfirm(confirmation);
    }
  }

  function rootCorrectionConfirmation(): AddGameConfirmation | null {
    if (selectedOption?.catalogAction !== 'correct_existing_root') {
      return null;
    }
    return {
      rootChoice: selectedOption.rootChoice,
      allowRootCorrection: true,
      chosenExecutable: chosenExecutable || null,
    };
  }

  function blockerMessage(blocker: RootCorrectionBlockerKind): string {
    switch (blocker) {
      case 'pending_recovery':
        return t('addGame.rootCorrection.blocker.pendingRecovery');
      case 'installed_addon':
        return t('addGame.rootCorrection.blocker.installedAddon');
      case 'nvapi':
        return t('addGame.rootCorrection.blocker.nvapi');
      case 'orphaned_component_baseline':
        return t('addGame.rootCorrection.blocker.orphanedComponentBaseline');
    }
  }

  function confirmStandard(): void {
    if (normalConfirmationBlocked) {
      return;
    }
    void onConfirm({
      rootChoice,
      allowRootCorrection: false,
      chosenExecutable: chosenExecutable || null,
    });
  }
</script>

<Dialog
  open
  onOpenChange={(open) => {
    if (!open && !busy) {
      onClose();
    }
  }}
>
  <DialogContent showCloseButton={!busy}>
    <DialogHeader>
      <DialogTitle>{t('addGame.title')}</DialogTitle>
      <DialogDescription>{t('addGame.reviewDescription')}</DialogDescription>
    </DialogHeader>

    <div class="grid max-h-[min(62vh,38rem)] gap-4 overflow-y-auto pe-1 text-sm">
      {#if dialogState.kind === 'review' && dialogState.errorPresentation !== null}
        <Alert
          variant={dialogState.errorPresentation.severity === 'warning' ? 'warning' : 'destructive'}
        >
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>{t('addGame.cannotAddTitle')}</AlertTitle>
          <AlertDescription>
            <p>{dialogState.errorPresentation.message}</p>
            {#if dialogState.errorPresentation.suggestedActions.length > 0}
              <ul>
                {#each dialogState.errorPresentation.suggestedActions as action (action.code)}
                  <li>{action.label}</li>
                {/each}
              </ul>
            {/if}
            {#if dialogState.errorPresentation.recoveryBundlePath !== undefined}
              <p class="break-all">
                {t('error.recoveryBundlePath', {
                  path: dialogState.errorPresentation.recoveryBundlePath,
                })}
              </p>
            {/if}
          </AlertDescription>
        </Alert>
      {/if}

      {#if hasRecommendedAlternative}
        <fieldset class="min-w-0" disabled={busy}>
          <legend id="add-game-install-root-label" class="mb-2 text-sm font-medium">
            {t('addGame.installRoot')}
          </legend>
          <RadioGroup
            bind:value={rootChoice}
            disabled={busy}
            aria-labelledby="add-game-install-root-label"
          >
            <Item variant="muted" size="sm" class="items-start">
              <RadioGroupItem
                id="add-game-recommended-root"
                value="recommended"
                class="peer mt-0.5"
              />
              <Label for="add-game-recommended-root">
                <span class="min-w-0">
                  <span class="block">{t('addGame.recommendedFolder')}</span>
                  <span class="block leading-normal font-normal break-all text-muted-foreground">
                    {recommendedRoot}
                  </span>
                </span>
              </Label>
            </Item>
            {#if selectedRootSelectable}
              <Item variant="muted" size="sm" class="items-start">
                <RadioGroupItem id="add-game-selected-root" value="selected" class="peer mt-0.5" />
                <Label for="add-game-selected-root">
                  <span class="min-w-0">
                    <span class="block">{t('addGame.selectedFolder')}</span>
                    <span class="block leading-normal font-normal break-all text-muted-foreground">
                      {inspection.selectedRoot}
                    </span>
                  </span>
                </Label>
              </Item>
            {:else}
              <Item variant="muted" size="sm">
                <ItemContent>
                  <ItemTitle>{t('addGame.selectedFolder')}</ItemTitle>
                  <ItemDescription class="line-clamp-none text-left text-wrap break-all">
                    {inspection.selectedRoot}
                  </ItemDescription>
                </ItemContent>
              </Item>
            {/if}
          </RadioGroup>
        </fieldset>
      {:else}
        <Item variant="muted" size="sm">
          <ItemContent>
            <ItemTitle>{t('addGame.selectedFolder')}</ItemTitle>
            <ItemDescription class="line-clamp-none text-left text-wrap break-all">
              {inspection.selectedRoot}
            </ItemDescription>
          </ItemContent>
        </Item>
        {#if recommendedRoot !== null}
          <Item variant="muted" size="sm">
            <ItemContent>
              <ItemTitle>{t('addGame.existingRoot')}</ItemTitle>
              <ItemDescription class="line-clamp-none text-left text-wrap break-all">
                {recommendedRoot}
              </ItemDescription>
            </ItemContent>
          </Item>
        {/if}
      {/if}

      {#each visibleWarnings as warning, index (`${warning.code}:${index}`)}
        <Alert variant="warning" size="sm" data-add-game-warning={warning.code}>
          <TriangleAlertIcon />
          <AlertDescription>{formatAddGameWarning(warning)}</AlertDescription>
        </Alert>
      {/each}

      {#each standaloneUnavailableReasons as reason (reason)}
        <Alert variant="destructive" size="sm" data-add-game-unavailable={reason}>
          <TriangleAlertIcon />
          <AlertDescription>{formatAddGameUnavailableReason(reason)}</AlertDescription>
        </Alert>
      {/each}

      {#if rootCorrection?.status === 'ready'}
        <Alert data-root-correction-status="ready">
          <InfoIcon />
          <AlertTitle>{t('addGame.replaceRootTitle')}</AlertTitle>
          <AlertDescription>{t('addGame.replaceRootDescription')}</AlertDescription>
        </Alert>
      {:else if rootCorrection?.status === 'cleanup_required'}
        <Alert variant="warning" data-root-correction-status="cleanup_required">
          <TriangleAlertIcon />
          <AlertTitle>{t('addGame.rootCorrection.rollbackTitle')}</AlertTitle>
          <AlertDescription>
            {t('addGame.rootCorrection.rollbackDescription', {
              count: rootCorrection.cleanupActions.length,
            })}
          </AlertDescription>
        </Alert>
      {:else if rootCorrection?.status === 'blocked'}
        {#each rootCorrection.blockers as blocker (blocker)}
          <Alert
            variant="warning"
            data-root-correction-status="blocked"
            data-root-correction-blocker={blocker}
          >
            <TriangleAlertIcon />
            <AlertDescription>{blockerMessage(blocker)}</AlertDescription>
          </Alert>
        {/each}
      {/if}

      {#if inspection.requiresExplicitExecutable}
        <div class="grid gap-2">
          <Label for="add-game-executable">{t('addGame.chooseExecutable')}</Label>
          <Select type="single" bind:value={chosenExecutable} disabled={busy}>
            <SelectTrigger id="add-game-executable" class="w-full">
              {chosenExecutableLabel}
            </SelectTrigger>
            <SelectContent>
              {#each validExecutables as candidate (candidate.path)}
                <SelectItem value={candidate.path} label={candidate.relativePath}>
                  {candidate.relativePath}
                </SelectItem>
              {/each}
            </SelectContent>
          </Select>
        </div>
      {/if}
    </div>

    <DialogFooter>
      <Button variant="secondary" size="sm" disabled={busy} onclick={onClose}>
        {t('common.cancel')}
      </Button>
      <Button variant="outline" size="sm" disabled={busy} onclick={() => void onChooseFolder()}>
        {t('addGame.chooseAnother')}
      </Button>
      {#if rootCorrection?.status === 'cleanup_required' && selectedOption?.catalogAction === 'correct_existing_root'}
        <Button
          size="sm"
          disabled={busy || requiresExecutableChoice}
          onclick={rollbackAndConfirmRootCorrection}
        >
          {#if busy}<Spinner />{/if}
          {t('addGame.rootCorrection.rollbackAndReplace')}
        </Button>
      {:else if rootCorrection?.status === 'ready' && selectedOption?.catalogAction === 'correct_existing_root'}
        <Button
          size="sm"
          disabled={busy || requiresExecutableChoice}
          onclick={confirmSelectedRootCorrection}
        >
          {#if busy}<Spinner />{/if}
          {inspection.relationship.kind === 'inside_existing'
            ? t('addGame.replaceExistingRoot')
            : t('addGame.correctRoot')}
        </Button>
      {:else if !normalConfirmationBlocked}
        <Button size="sm" disabled={busy} onclick={confirmStandard}>
          {#if busy}<Spinner />{/if}
          {selectedOption?.catalogAction === 'rescan' ? t('addGame.rescan') : t('addGame.add')}
        </Button>
      {/if}
    </DialogFooter>
  </DialogContent>
</Dialog>
