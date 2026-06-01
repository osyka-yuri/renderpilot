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
  'nav.operations': 'Операции',
  'nav.gameFallback': 'Игра',
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

  // ── Settings: tabs ──
  'settings.tabs.appearance': 'Оформление',
  'settings.tabs.catalog': 'Каталог',

  // ── Game card ──
  'game.card.action.details': 'Подробнее',
  'game.card.action.journal': 'Журнал',
  'game.card.action.detailsAria': 'Открыть подробности: {title}',
  'game.card.action.journalAria': 'Открыть журнал: {title}',
  'game.card.detectedLibraries': 'Найденные компоненты',
  'game.card.badge.upToDate': 'Актуально',
  'game.card.badge.updatesAvailable': 'Доступны обновления',
  'game.card.badge.updatesAvailableCount': {
    one: 'Доступно 1 обновление',
    few: 'Доступно {count} обновления',
    many: 'Доступно {count} обновлений',
    other: 'Доступно {count} обновлений',
  },
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
  'elevation.relaunchFailed': 'Не удалось перезапустить приложение',
  'elevation.dismiss': 'Скрыть',

  // ── Games page / catalog ──
  'games.scanFolder': 'Добавить папку',
  'games.scanning': 'Поиск игр...',
  'games.libraryActions': 'Действия',
  'games.search': 'Поиск игр',
  'games.openFilters': 'Фильтры',
  'games.openFiltersActive': 'Фильтры (активны)',
  'games.loading': 'Загрузка...',
  'games.empty.title': 'Игры не найдены',
  'games.empty.description': 'Укажите папку с играми, чтобы добавить их в список.',
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

  // ── Common ──
  'common.unknown': 'Неизвестно',

  // ── Game details: empty states ──
  'gameDetails.noGameSelected.title': 'Игра не выбрана',
  'gameDetails.noGameSelected.description': 'Выберите игру из списка для просмотра деталей.',
  'gameDetails.noComponents.title': 'Компоненты не найдены',
  'gameDetails.noComponents.description': 'У этой игры нет поддерживаемых графических компонентов.',

  // ── Game details: component version row ──
  'gameDetails.version.noReplacements': 'Нет альтернативных версий',
  'gameDetails.version.restoreOriginal': 'Восстановить {fileName}',

  // ── Game details: vendor component card ──
  'gameDetails.vendor.description': 'Изменить версию компонента.',

  // ── Game details: DLSS component card ──
  'gameDetails.dlss.description': 'Изменить версию DLSS или переопределить настройки.',
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
  'gameDetails.streamline.advanced': 'Дополнительные настройки ({count})',
  'gameDetails.streamline.perPluginWarning':
    'Рекомендуется использовать одну версию для всех плагинов. Меняйте отдельные плагины только при необходимости.',

  // ── Game details: NVIDIA profile card ──
  'gameDetails.profile.title': 'Профиль NVIDIA',
  'gameDetails.profile.description': 'Настройте параметры драйвера NVIDIA для этой игры.',
  'gameDetails.profile.target': 'Исполняемый файл',
  'gameDetails.profile.loading': 'Загрузка...',
  'gameDetails.profile.pinnedManual': 'Выбрано вручную.',
  'gameDetails.profile.autoDetected': 'Определено автоматически.',
  'gameDetails.profile.noExeDetected': 'Исполняемый файл не найден.',
  'gameDetails.profile.noExe': 'Файл не найден',
  'gameDetails.profile.autoDetect': 'Автоопределение',
  'gameDetails.profile.filteredTag': '(скрыто)',
  'gameDetails.profile.filteredLabel': '{fileName} (скрыто)',
  'gameDetails.profile.noProfile':
    'Профиль NVIDIA не найден. Запустите игру хотя бы раз и попробуйте снова.',

  // ── Game details: DLSS indicator card ──
  'gameDetails.indicator.title': 'Индикатор DLSS',
  'gameDetails.indicator.description': 'Показывать версию и настройки DLSS поверх игры.',
  'gameDetails.indicator.systemWide': 'Глобально',
  'gameDetails.indicator.adminRequired':
    'Перезапустите приложение от имени администратора для изменения этой настройки.',
  'gameDetails.indicator.overlayTitle': 'Экранный оверлей',
  'gameDetails.indicator.overlayDescription': 'Применяется ко всем играм на этом ПК.',
  'gameDetails.indicator.toggleAria': 'Переключить индикатор DLSS',

  // ── Game details: NVAPI setting row ──
  'gameDetails.nvapi.requiresDriver': 'требуется драйвер {version}+',
  'gameDetails.nvapi.unavailable': 'недоступно',
  'gameDetails.nvapi.resetDefault': 'Сбросить',
  'gameDetails.nvapi.alreadyDefault': 'Установлено по умолчанию',
  'gameDetails.nvapi.restoreBaselineAria': 'Восстановить исходное значение',
  'gameDetails.nvapi.restoreBaseline': 'Восстановить исходное значение',
  'gameDetails.nvapi.alreadyBaseline': 'Уже установлено исходное значение',
  'gameDetails.nvapi.noBaseline': 'Исходное значение не сохранено',

  // ── Operations page ──
  'operations.title': 'История',
  'operations.subtitleAll': 'Все действия',
  'operations.subtitleGame': 'Действия для {title}',
  'operations.viewGame': 'Открыть игру',
  'operations.loading': 'Загрузка...',
  'operations.empty': 'История пуста',
  'operations.historyAria': 'История действий',
  'operations.items': 'Элементы',

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

  // ── Operation presenters (status / kind / risk labels) ──
  'operation.label.low': 'Низкий риск',
  'operation.label.medium': 'Средний риск',
  'operation.label.high': 'Высокий риск',
  'operation.label.blocked': 'Заблокировано',
  'operation.label.planned': 'Запланировано',
  'operation.label.completed': 'Завершено',
  'operation.label.failed': 'Ошибка',
  'operation.label.rolledBack': 'Отменено',
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
  'notify.applyCompleted': 'Изменения применены',
  'notify.rollbackCompleted': 'Откат выполнен',
  'notify.statusError': 'Ошибка',
  'notify.statusWarning': 'Внимание',

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
  'suggested_action.normalize_text': 'Проверьте введенные данные и попробуйте снова.',
  'suggested_action.inspect_logs':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.retry_or_restart':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.rebuild_operation_plan': 'Пожалуйста, начните действие заново.',
  'suggested_action.refresh_or_scan_game_folder': 'Обновите список или отсканируйте папку заново.',
  'suggested_action.relaunch_as_administrator':
    'Перезапустите приложение от имени администратора и попробуйте снова.',
  // ── NVAPI Settings (Dynamic Fallbacks) ──
  'nvapi.dlss_sr_render_preset.label': 'Пресет рендеринга',
  'nvapi.dlss_sr_render_preset.description':
    'Принудительно задает пресет DLSS. В некоторых играх также требуется изменить "Профиль пресета" (Model Preset Profile).',
  'nvapi.dlss_sr_render_preset.value.default': 'Выкл (как в игре)',
  'nvapi.dlss_sr_render_preset.value.recommended': 'Рекомендовано',

  'nvapi.dlss_sr_quality_level.label': 'Уровень качества',
  'nvapi.dlss_sr_quality_level.description':
    'Переопределяет выбранный в игре уровень качества DLSS.',
  'nvapi.dlss_sr_quality_level.value.custom': 'Свой',
  'nvapi.dlss_sr_quality_level.value.na': 'Н/Д',

  'nvapi.dlss_sr_scaling_ratio.label': 'Масштаб рендеринга',
  'nvapi.dlss_sr_scaling_ratio.description':
    'Задает свой масштаб (разрешение) для рендеринга. Требует установить "Уровень качества" в режим "Свой".',

  'nvapi.dlss_sr_dll_override.label': 'Переопределение версии DLL',
  'nvapi.dlss_sr_dll_override.description':
    'Заставляет игру использовать последнюю системную версию DLSS.',
  'nvapi.dlss_sr_dll_override.value.off': 'Выкл',
  'nvapi.dlss_sr_dll_override.value.on': 'Вкл (последняя системная)',

  'nvapi.dlss_sr_model_preset_profile.label': 'Профиль пресета',
  'nvapi.dlss_sr_model_preset_profile.description':
    'Позволяет применить кастомный пресет в играх, где настройка "Пресет рендеринга" не работает сама по себе.',
  'nvapi.dlss_sr_model_preset_profile.value.na': 'Н/Д',
  'nvapi.dlss_sr_model_preset_profile.value.recommended': 'Рекомендовано',
  'nvapi.dlss_sr_model_preset_profile.value.custom': 'Свой',

  'nvapi.dlss_sr_override_dlaa.label': 'Принудительный DLAA',
  'nvapi.dlss_sr_override_dlaa.description':
    'Заставляет все режимы качества DLSS рендериться в полном разрешении (как DLAA).',
  'nvapi.dlss_sr_override_dlaa.value.off': 'Выкл',
  'nvapi.dlss_sr_override_dlaa.value.on': 'Вкл',

  'nvapi.dlss_sr_override_perf_to_ultraperf.label': 'Performance в Ultra Performance',
  'nvapi.dlss_sr_override_perf_to_ultraperf.description':
    'Заставляет режим качества "Performance" использовать алгоритм рендеринга "Ultra Performance".',
  'nvapi.dlss_sr_override_perf_to_ultraperf.value.off': 'Выкл',
  'nvapi.dlss_sr_override_perf_to_ultraperf.value.on': 'Вкл',

  'nvapi.dlss_fg_render_preset.label': 'Пресет рендеринга',
  'nvapi.dlss_fg_render_preset.description':
    'Принудительно задает пресет генерации кадров (Frame Generation).',
  'nvapi.dlss_fg_render_preset.value.default': 'Выкл (как в игре)',
  'nvapi.dlss_fg_render_preset.value.recommended': 'Рекомендовано',

  'nvapi.dlss_fg_mode.label': 'Режим работы',
  'nvapi.dlss_fg_mode.description':
    'Устанавливает режим генерации кадров. Для динамического режима требуется драйвер 595.97 и новее.',
  'nvapi.dlss_fg_mode.value.na': 'Н/Д',
  'nvapi.dlss_fg_mode.value.fixed': 'Фиксированный',
  'nvapi.dlss_fg_mode.value.dynamic': 'Динамический',

  'nvapi.dlss_fg_dll_override.label': 'Переопределение версии DLL',
  'nvapi.dlss_fg_dll_override.description':
    'Заставляет игру использовать последнюю системную версию Frame Generation.',
  'nvapi.dlss_fg_dll_override.value.off': 'Выкл',
  'nvapi.dlss_fg_dll_override.value.on': 'Вкл (последняя системная)',

  'nvapi.dlss_mfg_fixed_count.label': 'Генерация кадров — Фиксированное кол-во',
  'nvapi.dlss_mfg_fixed_count.description':
    'Задает точное количество генерируемых кадров на каждый отрисованный кадр.',
  'nvapi.dlss_mfg_fixed_count.value.na': 'Н/Д',

  'nvapi.dlss_mfg_dynamic_count.label': 'Генерация кадров — Динамическое кол-во',
  'nvapi.dlss_mfg_dynamic_count.description':
    'Устанавливает верхний предел генерации кадров для динамического режима.',
  'nvapi.dlss_mfg_dynamic_count.value.na': 'Н/Д',

  'nvapi.dlss_mfg_target_frame_rate.label': 'Генерация кадров — Целевая частота (FPS)',
  'nvapi.dlss_mfg_target_frame_rate.description':
    'Задает частоту кадров, которую будет пытаться поддерживать динамический режим.',
  'nvapi.dlss_mfg_target_frame_rate.value.na': 'Н/Д',
  'nvapi.dlss_mfg_target_frame_rate.value.max_refresh': 'Макс. частота обновления монитора',

  'nvapi.dlss_rr_render_preset.label': 'Пресет рендеринга',
  'nvapi.dlss_rr_render_preset.description':
    'Принудительно задает пресет реконструкции лучей (Ray Reconstruction).',
  'nvapi.dlss_rr_render_preset.value.default': 'Выкл (как в игре)',
  'nvapi.dlss_rr_render_preset.value.recommended': 'Рекомендовано',

  'nvapi.dlss_rr_quality_level.label': 'Уровень качества',
  'nvapi.dlss_rr_quality_level.description':
    'Переопределяет выбранный в игре уровень качества Ray Reconstruction.',
  'nvapi.dlss_rr_quality_level.value.custom': 'Свой',
  'nvapi.dlss_rr_quality_level.value.na': 'Н/Д',

  'nvapi.dlss_rr_scaling_ratio.label': 'Масштаб рендеринга',
  'nvapi.dlss_rr_scaling_ratio.description':
    'Задает свой масштаб (разрешение) для рендеринга. Требует установить "Уровень качества" в режим "Свой".',

  'nvapi.dlss_rr_dll_override.label': 'Переопределение версии DLL',
  'nvapi.dlss_rr_dll_override.description':
    'Заставляет игру использовать последнюю системную версию Ray Reconstruction.',
  'nvapi.dlss_rr_dll_override.value.off': 'Выкл',
  'nvapi.dlss_rr_dll_override.value.on': 'Вкл (последняя системная)',
};
