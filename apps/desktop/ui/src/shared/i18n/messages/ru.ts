import type { MessageKey } from './en';
import type { MessageValue } from './types';

/**
 * Russian catalog. Typed as `Record<MessageKey, MessageValue>` so a missing or
 * stray key is a compile-time error against the English source of truth.
 */
export const ru: Record<MessageKey, MessageValue> = {
  // ── App shell / navigation ──
  'nav.games': 'Игры',
  'nav.libraries': 'Библиотеки',
  'nav.settings': 'Настройки',
  'nav.operations': 'Журнал',
  'nav.gameFallback': 'Игра',
  'nav.donate': 'Поддержать',
  'shell.refresh': 'Обновить',

  // ── Settings: appearance section ──
  'settings.appearance.title': 'Оформление',
  'settings.appearance.description': 'Настройте внешний вид приложения и язык.',
  'settings.appearance.theme.title': 'Тема',
  'settings.appearance.theme.description': 'Выберите цветовую тему приложения.',
  'settings.appearance.theme.triggerLabel': 'Тема',
  'settings.appearance.theme.placeholder': 'Выберите тему',
  'settings.appearance.language.title': 'Язык',
  'settings.appearance.language.description': 'Выберите язык интерфейса.',
  'settings.appearance.language.triggerLabel': 'Язык',
  'settings.appearance.language.placeholder': 'Выберите язык',

  // ── Settings: theme options ──
  'settings.theme.system': 'Системная',
  'settings.theme.dark': 'Тёмная',
  'settings.theme.light': 'Светлая',

  // ── Settings: language options (en/ru labels are endonyms — identical in every locale) ──
  'settings.language.system': 'Как в системе',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zh': '中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  // ── Settings: tabs ──
  'settings.tabs.general': 'Общие',
  'settings.tabs.catalog': 'Каталог',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'Индикатор DLSS',
  'settings.nvidia.indicator.description': 'Показывать версию и настройки DLSS поверх игры.',
  'settings.nvidia.indicator.systemWide': 'Глобально',
  'settings.nvidia.indicator.adminRequired':
    'Перезапустите приложение от имени администратора для изменения этой настройки.',
  'settings.nvidia.indicator.overlayTitle': 'Экранный оверлей',
  'settings.nvidia.indicator.overlayDescription': 'Применяется ко всем играм на этом ПК.',
  'settings.nvidia.indicator.toggleAria': 'Переключить индикатор DLSS',

  // ── Game card ──
  'game.card.action.details': 'Подробнее',
  'game.card.action.detailsAria': 'Открыть подробности: {title}',
  'game.card.detectedLibraries': 'Найденные компоненты',
  'game.card.badge.upToDate': 'Актуально',
  'game.card.badge.updatesAvailable': 'Доступны обновления',
  'game.card.badge.updatesAvailableCount': {
    one: 'Доступно 1 обновление',
    few: 'Доступно {count} обновления',
    many: 'Доступно {count} обновлений',
    other: 'Доступно {count} обновлений',
  },
  'game.card.badge.hidden': 'Скрыто',
  'game.card.menu.ariaLabel': 'Параметры для {title}',
  'game.card.menu.favorite.add': 'Добавить в избранное',
  'game.card.menu.favorite.remove': 'Убрать из избранного',
  'game.card.menu.favorite.toggleHint': 'Переключить статус "избранное" для этой игры.',
  'game.card.menu.hidden.add': 'Скрыть игру',
  'game.card.menu.hidden.remove': 'Показать игру',
  'game.card.menu.hidden.toggleHint': 'Переключить статус видимости для этой игры.',
  'game.libraries.empty': 'Компоненты не найдены',

  // ── Game cover ──
  'game.cover.alt': 'Обложка',
  'game.cover.altWithTitle': 'Обложка: {title}',
  'game.cover.menu.ariaLabel': 'Параметры обложки для {title}',
  'game.cover.menu.fetch': 'Скачать обложку',
  'game.cover.menu.fetching': 'Скачивание…',
  'game.cover.menu.fetchHint': 'Найти и скачать обложку из интернета.',
  'game.cover.menu.pick': 'Выбрать файл изображения…',
  'game.cover.menu.pickHint': 'Выбрать изображение на компьютере.',
  'game.cover.menu.clear': 'Удалить обложку',
  'game.cover.menu.clearHint': 'Вернуть стандартную обложку.',

  // ── Games dashboard summary ──
  'game.dashboard.summary': 'Сводка',
  'game.dashboard.games': {
    one: '{count} игра',
    few: '{count} игры',
    many: '{count} игр',
    other: '{count} игр',
  },
  'game.dashboard.updates': {
    one: '{count} обновление',
    few: '{count} обновления',
    many: '{count} обновлений',
    other: '{count} обновлений',
  },
  'game.dashboard.rollbacksReady': {
    one: '{count} откат доступен',
    few: '{count} отката доступно',
    many: '{count} откатов доступно',
    other: '{count} откатов доступно',
  },

  // ── Elevation banner ──
  'elevation.title': 'Требуются права администратора',
  'elevation.description': 'Для изменения некоторых настроек требуются права администратора.',
  'elevation.relaunch': 'Перезапустить от имени администратора',
  'elevation.relaunchFailed': 'Не удалось перезапустить от имени администратора',
  'elevation.dismiss': 'Скрыть',
  'error.boundary.title': 'Что-то пошло не так',
  'error.boundary.description':
    'На этом экране произошла непредвиденная ошибка. Попробуйте ещё раз или перейдите в другой раздел.',
  'error.boundary.reset': 'Повторить',

  // ── Games page / catalog ──
  'games.scanFolder': 'Сканировать папку',
  'games.scanning': 'Поиск игр...',
  'games.libraryActions': 'Действия',
  'games.search': 'Поиск игр',
  'games.openFilters': 'Фильтры',
  'games.openFiltersActive': 'Фильтры (активны)',
  'games.loading': 'Загрузка...',
  'games.empty.title': 'Игры не найдены',
  'games.empty.description': 'Просканируйте папку, чтобы добавить игры в список.',
  'games.filterEmpty.title': 'Ничего не найдено',
  'games.filterEmpty.description': 'Попробуйте изменить условия поиска или фильтры.',
  'games.filterEmpty.reset': 'Сбросить фильтры',

  // ── Settings: catalog (cover sources) ──
  'settings.catalog.title': 'Источники обложек',
  'settings.catalog.description': 'Выберите, откуда скачивать обложки для игр.',
  'settings.catalog.steamKey.srLabel': 'API-ключ SteamGridDB',
  'settings.catalog.steamKey.placeholder': 'API-ключ',
  'settings.catalog.steamKey.loading': 'Загрузка…',
  'settings.catalog.steamKey.save': 'Сохранить',
  'settings.catalog.steamKey.saved': 'Сохранено',
  'settings.catalog.steamKey.cleared': 'Ключ удалён',
  'settings.catalog.steamKey.readError': 'Не удалось прочитать настройки.',
  'settings.catalog.steamKey.saveError': 'Не удалось сохранить настройки.',

  // ── Settings: about ──
  'settings.about.title': 'Обновления',
  'settings.about.description': 'Проверьте наличие новых версий приложения.',
  'settings.about.version.title': 'Версия приложения',
  'settings.about.version.loading': 'Определение...',
  'settings.about.checkUpdates': 'Проверить обновления',
  'settings.about.downloading': 'Скачивание...',
  'settings.about.updateAvailable': 'Доступно обновление: {version}. Установить и перезапустить?',
  'settings.about.upToDate': 'У вас установлена последняя версия',
  'settings.about.updateError': 'Не удалось проверить обновления',

  // ── Common ──
  'common.unknown': 'Неизвестно',
  'common.downloadProgress': 'Прогресс скачивания',

  // ── Game details: empty states ──
  'gameDetails.noGameSelected.title': 'Игра не выбрана',
  'gameDetails.noGameSelected.description': 'Выберите игру из списка для просмотра деталей.',
  'gameDetails.noComponents.title': 'Компоненты не найдены',
  'gameDetails.noComponents.description': 'У этой игры нет поддерживаемых графических компонентов.',

  // ── Game details: component version row ──
  'gameDetails.version.noReplacements': 'Нет альтернативных версий',
  'gameDetails.version.restoreOriginal': 'Восстановить исходный {fileName}',

  // ── Game details: vendor component card ──
  'gameDetails.vendor.description': 'Изменить версию компонента.',

  // ── Game details: DLSS component card ──
  'gameDetails.dlss.description': 'Изменить версию DLSS или переопределить настройки.',
  'gameDetails.dlss.descriptionSwapOnly': 'Изменить версию DLSS.',
  'gameDetails.dlss.libraryFileLabel': 'Версия файла',
  'gameDetails.dlss.driverOverridesLabel': 'Настройки драйвера NVIDIA',
  'gameDetails.dlss.adminRequired':
    'Перезапустите приложение от имени администратора для изменения этих настроек.',

  // ── Game details: Streamline card ──
  'gameDetails.streamline.description': 'Управление плагинами Streamline.',
  'gameDetails.streamline.versionTitle': 'Общая версия Streamline',
  'gameDetails.streamline.versionDescription': 'Применяет одну версию ко всем плагинам.',
  'gameDetails.streamline.noOtherVersions': 'Других версий нет',
  'gameDetails.streamline.mixed': 'Разные версии',
  'gameDetails.streamline.updatesSummary': 'обновлений: {updates} · отсутствует: {missing}',
  'gameDetails.streamline.restoreAllAria': 'Восстановить исходные версии',
  'gameDetails.streamline.restoreAllTooltip': 'Восстановить исходные',
  'gameDetails.streamline.mixedWarning':
    'Плагины используют разные версии. Выберите версию выше для синхронизации.',

  // ── Game details: NVIDIA profile card ──
  'gameDetails.profile.title': 'Профиль NVIDIA',
  'gameDetails.profile.description': 'Настройте параметры драйвера NVIDIA для этой игры.',
  'gameDetails.profile.target': 'Исполняемый файл',
  'gameDetails.profile.loading': 'Загрузка...',
  'gameDetails.profile.pinnedManual': 'Выбрано вручную.',
  'gameDetails.profile.autoDetected': 'Определено автоматически.',
  'gameDetails.profile.noExeDetected': 'Исполняемый файл не найден.',
  'gameDetails.profile.noExe': 'Нет файла',
  'gameDetails.profile.autoDetect': 'Автоопределение',
  'gameDetails.profile.filteredTag': '(скрыто)',
  'gameDetails.profile.filteredLabel': '{fileName} (скрыто)',
  'gameDetails.profile.noProfile': 'Профиль NVIDIA не найден.',

  // ── Game details: NVAPI setting row ──
  'gameDetails.nvapi.requiresDriver': 'требуется драйвер {version}+',
  'gameDetails.nvapi.unavailable': 'недоступно',
  'gameDetails.nvapi.resetDefault': 'Сбросить',
  'gameDetails.nvapi.alreadyDefault': 'Установлено по умолчанию',
  'gameDetails.nvapi.restoreBaselineAria': 'Восстановить исходное значение',
  'gameDetails.nvapi.restoreBaseline': 'Восстановить исходное значение',
  'gameDetails.nvapi.alreadyBaseline': 'Уже установлено исходное значение',
  'gameDetails.nvapi.noBaseline': 'Исходное значение не сохранено',

  'gameDetails.nvapi.warning.noDll': 'DLL-файл DLSS не найден в папке с игрой.',
  'gameDetails.nvapi.warning.noManifest': 'В манифесте нет данных для этой версии DLL.',
  'gameDetails.nvapi.warning.noExecutable': 'Исполняемый файл для этой игры не найден.',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI недоступен.',
  'gameDetails.nvapi.warning.nvapiInitFailed': 'Ошибка инициализации NVAPI.',
  'gameDetails.nvapi.warning.drsFailed': 'Не удалось создать сессию DRS.',

  // ── Operations page ──
  'operations.title': 'Журнал операций',
  'operations.subtitleGame': 'Операции для {title}',
  'operations.loading': 'Загрузка...',
  'operations.empty': 'Операций нет',
  'operations.gameName': 'Игра',
  'operations.date': 'Дата',
  'operations.status': 'Статус',
  'operations.action': 'Действие',
  'operations.libraryType': 'Тип библиотеки',
  'operations.version': 'Версия',

  // ── Libraries page ──
  'libraries.error': 'Ошибка',
  'libraries.hash.copy': 'Скопировать хеш',
  'libraries.hash.copied': 'Скопировано',
  'libraries.hash.failed': 'Не удалось скопировать',
  'libraries.hash.copiedToast': 'Хеш скопирован в буфер обмена',
  'libraries.sort.asc': 'Сортировка по возрастанию',
  'libraries.sort.desc': 'Сортировка по убыванию',
  'libraries.sort.none': 'Без сортировки',
  'libraries.actions.delete': 'Удалить',
  'libraries.actions.download': 'Скачать',
  'libraries.actions.deletedToast': 'Удалено {version}',
  'libraries.actions.downloadedToast': 'Скачано {version}',
  'libraries.actions.failedToast': 'Не удалось выполнить: {action}',

  // ── Common actions ──
  'common.cancel': 'Отмена',
  'common.apply': 'Применить',

  // ── Filter games ──
  'filters.title': 'Фильтры',
  'filters.launchers.title': 'Лаунчеры',
  'filters.launchers.empty': 'Лаунчеры не найдены',
  'filters.launchers.reorder': 'Переместить {label}',
  'filters.libraries.title': 'Компоненты',
  'filters.libraries.empty': 'Компоненты не найдены',

  // ── Games toolbar ──
  'games.favoritesToggle': 'Избранное',
  'games.favoritesToggleActive': 'Избранное (активно)',
  'games.showHidden': 'Скрытые игры',
  'games.showHiddenActive': 'Скрытые игры (активно)',

  // ── Operation presenters (status / kind / risk labels) ──
  'operation.label.low': 'Низкий риск',
  'operation.label.medium': 'Средний риск',
  'operation.label.high': 'Высокий риск',
  'operation.label.blocked': 'Заблокировано',
  'operation.label.planned': 'Запланировано',
  'operation.label.completed': 'Завершено',
  'operation.label.failed': 'Ошибка',
  'operation.label.rolledBack': 'Откат',
  'operation.label.replaceComponent': 'Изменение версии',
  'operation.duration': 'Выполнено за {seconds} с',
  'operation.filesUpdated.none': 'Файлы не обновлялись.',
  'operation.filesUpdated.count': {
    one: 'Обновлён 1 файл.',
    few: 'Обновлено {count} файла.',
    many: 'Обновлено {count} файлов.',
    other: 'Обновлено {count} файлов.',
  },
  'operation.filesRestored.none': 'Файлы не восстанавливались.',
  'operation.filesRestored.count': {
    one: 'Восстановлен 1 файл.',
    few: 'Восстановлено {count} файла.',
    many: 'Восстановлено {count} файлов.',
    other: 'Восстановлено {count} файлов.',
  },
  'operation.itemAria': '{kind}, {status}',

  // ── Notifications (toasts) ──
  'notify.stalePlan': 'План операции устарел. Попробуйте снова.',
  'notify.missingStableGameId': 'Не удалось идентифицировать игру.',
  'notify.coverPickerPreview': 'Для выбора обложки используйте десктопное приложение.',
  'notify.coverUpdated.title': 'Обложка обновлена',
  'notify.coverUpdated.body': 'Пользовательское изображение сохранено.',
  'notify.coverDownloaded.title': 'Обложка скачана',
  'notify.coverDownloaded.body': 'Обложка игры обновлена.',
  'notify.coverRemoved.title': 'Обложка удалена',
  'notify.coverRemoved.body': 'Возвращена стандартная обложка.',
  'notify.favoriteFailed': 'Не удалось изменить статус избранного.',
  'notify.favoriteAdded': 'Добавлено в избранное.',
  'notify.favoriteRemoved': 'Убрано из избранного.',
  'notify.hiddenFailed': 'Не удалось изменить видимость игры.',
  'notify.gameHidden': 'Игра скрыта.',
  'notify.gameUnhidden': 'Игра теперь отображается.',
  'notify.applyCompleted': 'Изменения применены',
  'notify.rollbackCompleted': 'Откат выполнен',
  'notify.statusError': 'Ошибка',
  'notify.statusWarning': 'Предупреждение',

  // ── Library scan ──
  'scan.partialWarning': {
    one: 'Не удалось просканировать 1 папку.',
    few: 'Не удалось просканировать {count} папки.',
    many: 'Не удалось просканировать {count} папок.',
    other: 'Не удалось просканировать {count} папок.',
  },

  // ── Background cover sync ──
  'coverSync.failed': 'Не удалось синхронизировать обложки.',
  'coverSync.refreshFailed': 'Не удалось синхронизировать обложки.',

  // ── NVIDIA driver context (toasts) ──
  'nvidia.adminRequired': 'Требуются права администратора',
  'nvidia.relaunchTo': 'Перезапустите приложение от имени администратора, чтобы {action}.',
  'nvidia.action.changeSetting': 'применить настройки',
  'nvidia.action.revertSetting': 'сбросить настройки',
  'nvidia.changeSettingFailed': 'Не удалось применить настройки',
  'nvidia.revertDefaultFailed': 'Не удалось сбросить настройки',
  'nvidia.revertBaselineFailed': 'Не удалось восстановить исходные настройки',
  'nvidia.setExeFailed': 'Не удалось настроить исполняемый файл',
  'nvidia.clearExeFailed': 'Не удалось сбросить настройки файла',

  // ── DLSS indicator context (toasts) ──
  'indicator.relaunchToToggle':
    'Перезапустите приложение от имени администратора для переключения индикатора.',
  'indicator.changeFailed': 'Не удалось переключить индикатор DLSS',

  // ── Libraries table ──
  'libraries.column.version': 'Версия',
  'libraries.column.hash': 'Хеш',
  'libraries.column.signed': 'Подпись',
  'libraries.column.size': 'Размер',
  'libraries.column.actions': 'Действия',
  'libraries.unsigned': 'Без подписи',
  'libraries.invalidDate': 'Неверная дата',
  'libraries.empty.loading': 'Загрузка…',
  'libraries.empty.unavailable': 'Не удалось загрузить библиотеки',
  'libraries.empty.none': 'Библиотеки не найдены',
  'libraries.error.loadFailed': 'Не удалось загрузить библиотеки',
  'libraries.error.refreshFailed': 'Не удалось обновить манифест',
  'libraries.error.downloadFailed': 'Не удалось скачать',
  'libraries.error.deleteFailed': 'Не удалось удалить',
  'libraries.error.downloadedRefreshFailed': 'Библиотека скачана, но обновить статус не удалось',
  'libraries.error.deletedRefreshFailed': 'Библиотека удалена, но обновить статус не удалось',

  // ── Settings: cover source rows ──
  'settings.catalog.source.steam.aria': 'Скачивать обложки из Steam',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description': 'Скачивать обложки из публичного каталога Steam.',
  'settings.catalog.source.gog.aria': 'Скачивать обложки из GOG',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description': 'Скачивать обложки из официального каталога GOG.',
  'settings.catalog.source.steamgriddb.aria': 'Скачивать обложки из SteamGridDB',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'Скачивать обложки от сообщества. Требуется API-ключ.',
  'settings.catalog.artworkReadError': 'Не удалось загрузить настройки обложек.',
  'settings.catalog.artworkSaveError': 'Не удалось сохранить настройки обложек.',

  // ── Backend user messages (mirror of src-tauri/commands/error/strings.rs) ──
  'user_message.invalid_argument': 'Указано неверное значение.',
  'user_message.invalid_game_reference': 'Игра не найдена.',
  'user_message.invalid_component_reference': 'Компонент не найден.',
  'user_message.invalid_artifact_reference': 'Объект не найден.',
  'user_message.invalid_operation_reference': 'Действие не найдено.',
  'user_message.missing_required_info': 'Не хватает данных для выполнения.',
  'user_message.unexpected_input': 'Получены неожиданные данные.',
  'user_message.unrecognized_option': 'Указан неизвестный параметр.',
  'user_message.unsupported_technology_filter': 'Этот фильтр не поддерживается.',
  'user_message.non_unicode_input': 'Текст содержит недопустимые символы.',
  'user_message.response_serialization_failed': 'Не удалось обработать запрос.',
  'user_message.plan_changed_rebuild': 'Задача устарела. Попробуйте снова.',
  'user_message.game_not_in_catalog': 'Игра не поддерживается.',
  'user_message.operation_not_found': 'Действие не найдено.',
  'user_message.artifact_not_found': 'Объект не найден.',
  'user_message.component_not_found': 'Компонент не найден.',
  'user_message.invalid_operation_state': 'Это действие сейчас недоступно.',
  'user_message.operation_could_not_complete': 'Не удалось выполнить действие.',
  'user_message.command_task_failed': 'Не удалось выполнить команду.',
  'user_message.storage_failed': 'Не удалось прочитать или записать каталог приложения.',
  'user_message.provider_failed': 'Не удалось получить данные из источника.',
  'user_message.detection_failed': 'Не удалось проанализировать файлы игры.',
  'user_message.steamgriddb_api_key_missing': 'Укажите API-ключ SteamGridDB в настройках.',
  'user_message.unsupported_cover_image_type': 'Неподдерживаемый формат изображения.',
  'user_message.cover_download_failed': 'Не удалось скачать обложку.',
  'user_message.cover_artwork_not_found': 'Обложка для этой игры не найдена.',
  'user_message.cover_file_system_error': 'Не удалось сохранить обложку на диск.',
  'user_message.nvapi_requires_administrator':
    'Для изменения этой настройки требуются права администратора.',

  // ── Backend suggested actions ──
  'suggested_action.refresh_games': 'Обновите список игр и попробуйте снова.',
  'suggested_action.reload_game_details': 'Обновите информацию об игре и попробуйте снова.',
  'suggested_action.refresh_candidates': 'Обновите список и попробуйте снова.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Обновите страницу и попробуйте снова.',
  'suggested_action.retry_after_required_data': 'Подождите немного и попробуйте снова.',
  'suggested_action.reload_desktop': 'Перезапустите приложение и попробуйте снова.',
  'suggested_action.normalize_text': 'Проверьте введённые данные и попробуйте снова.',
  'suggested_action.inspect_logs':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.retry_or_restart':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.rebuild_operation_plan': 'Пожалуйста, начните действие заново.',
  'suggested_action.refresh_or_scan_game_folder': 'Обновите список или отсканируйте папку заново.',
  'suggested_action.relaunch_as_administrator':
    'Перезапустите приложение от имени администратора и попробуйте снова.',
};
