import type { CatalogSettingPayload } from '@entities/settings';
import type { DisposableRequestChannel } from '@shared/requests';
import { t } from '@shared/i18n';
import { steamGridDbSettingKey } from './steam-key-model';

type GetCatalogSetting = (key: string) => Promise<CatalogSettingPayload>;
type SetCatalogSetting = (key: string, value: string) => Promise<unknown>;

type SteamKeyStateChannel = {
  readInput: () => string;
  writeInput: (value: string) => void;
  setBusy: (busy: boolean) => void;
  setLoaded: (loaded: boolean) => void;
  setMessage: (message: string) => void;
};

export type SteamKeyControllerContext = {
  request: DisposableRequestChannel;
  getCatalogSetting: GetCatalogSetting;
  setCatalogSetting: SetCatalogSetting;
  state: SteamKeyStateChannel;
};

export async function loadSteamGridDbKey(context: SteamKeyControllerContext): Promise<void> {
  const requestId = beginWritableRequest(context.request);

  if (requestId === null) {
    return;
  }

  context.state.setBusy(true);
  context.state.setMessage('');

  try {
    const payload = await context.getCatalogSetting(steamGridDbSettingKey);

    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.writeInput(payload.value ?? '');
      context.state.setLoaded(true);
    }
  } catch {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setMessage(t('settings.catalog.steamKey.readError'));
      context.state.setLoaded(true);
    }
  } finally {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setBusy(false);
    }
  }
}

export async function saveSteamGridDbKey(context: SteamKeyControllerContext): Promise<void> {
  const requestId = beginWritableRequest(context.request);

  if (requestId === null) {
    return;
  }

  const normalizedKey = context.state.readInput().trim();

  context.state.setBusy(true);
  context.state.setMessage('');

  try {
    await context.setCatalogSetting(steamGridDbSettingKey, normalizedKey);

    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.writeInput(normalizedKey);
      context.state.setMessage(
        normalizedKey
          ? t('settings.catalog.steamKey.saved')
          : t('settings.catalog.steamKey.cleared'),
      );
    }
  } catch {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setMessage(t('settings.catalog.steamKey.saveError'));
    }
  } finally {
    if (canWriteLatestRequest(context.request, requestId)) {
      context.state.setBusy(false);
    }
  }
}

function beginWritableRequest(request: DisposableRequestChannel): number | null {
  if (request.isDisposed()) {
    return null;
  }

  const requestId = request.begin();

  if (request.isDisposed()) {
    return null;
  }

  return requestId;
}

function canWriteLatestRequest(request: DisposableRequestChannel, requestId: number): boolean {
  return !request.isDisposed() && request.isActive(requestId);
}
