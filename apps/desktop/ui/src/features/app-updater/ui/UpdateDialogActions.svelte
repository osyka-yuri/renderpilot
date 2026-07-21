<script lang="ts">
  import type { MessageKey } from '@shared/i18n';
  import { t } from '@shared/i18n';
  import { Button } from '@shared/ui';

  import type { UpdateDialogFooter } from '../model/dialog-view';

  type Props = {
    footer: UpdateDialogFooter;
    onInstall: () => void;
    onRetry: () => void;
    onDismiss: () => void;
    onRestart: () => void;
  };

  const { footer, onInstall, onRetry, onDismiss, onRestart }: Props = $props();

  type ActionView = {
    secondaryKey: MessageKey;
    primaryKey: MessageKey;
    onPrimary: () => void;
  };

  const view = $derived.by((): ActionView => {
    switch (footer.kind) {
      case 'install':
        return {
          secondaryKey: 'settings.about.updateDialog.later',
          primaryKey: 'settings.about.updateDialog.installAndRestart',
          onPrimary: onInstall,
        };
      case 'retry-download':
        return {
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.retryDownload',
          onPrimary: onRetry,
        };
      case 'retry-install':
        return {
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.retryInstall',
          onPrimary: onRetry,
        };
      case 'restart':
        return {
          secondaryKey: 'settings.about.updateDialog.close',
          primaryKey: 'settings.about.updateDialog.restartNow',
          onPrimary: onRestart,
        };
    }
  });
</script>

<Button variant="secondary" size="sm" onclick={onDismiss}>
  {t(view.secondaryKey)}
</Button>
<Button size="sm" onclick={view.onPrimary}>
  {t(view.primaryKey)}
</Button>
