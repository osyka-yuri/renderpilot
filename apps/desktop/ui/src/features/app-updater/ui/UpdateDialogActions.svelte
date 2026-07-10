<script lang="ts">
  import type { MessageKey } from '@shared/i18n';
  import { t } from '@shared/i18n';
  import { Button } from '@shared/ui';

  import { phaseStatusKey, type UpdateDialogFooter } from '../model/dialog-view';

  type Props = {
    footer: UpdateDialogFooter;
    onInstall: () => void;
    onRetry: () => void;
    onDismiss: () => void;
    onRestart: () => void;
  };

  const { footer, onInstall, onRetry, onDismiss, onRestart }: Props = $props();

  type ActionView =
    | { kind: 'busy'; phaseLabel: MessageKey }
    | {
        kind: 'pair';
        secondaryKey: MessageKey;
        primaryKey: MessageKey;
        onPrimary: () => void;
      }
    | { kind: 'none' };

  const view = $derived.by((): ActionView => {
    switch (footer.kind) {
      case 'install':
        return {
          kind: 'pair',
          secondaryKey: 'settings.about.updateDialog.later',
          primaryKey: 'settings.about.updateDialog.installAndRestart',
          onPrimary: onInstall,
        };
      case 'busy':
        return { kind: 'busy', phaseLabel: phaseStatusKey(footer.phase) };
      case 'retry-download':
        return {
          kind: 'pair',
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.retryDownload',
          onPrimary: onRetry,
        };
      case 'retry-install':
        return {
          kind: 'pair',
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.retryInstall',
          onPrimary: onRetry,
        };
      case 'restart':
        return {
          kind: 'pair',
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.restartNow',
          onPrimary: onRestart,
        };
      default:
        return { kind: 'none' };
    }
  });
</script>

{#if view.kind === 'busy'}
  <Button size="sm" disabled>
    {t(view.phaseLabel)}
  </Button>
{:else if view.kind === 'pair'}
  <Button variant="secondary" size="sm" onclick={onDismiss}>
    {t(view.secondaryKey)}
  </Button>
  <Button size="sm" onclick={view.onPrimary}>
    {t(view.primaryKey)}
  </Button>
{/if}
