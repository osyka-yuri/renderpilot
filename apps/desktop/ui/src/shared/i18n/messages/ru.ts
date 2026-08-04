import type { EnglishCatalog } from './en';
import { defineLocalizedCatalog } from './contract';
import { plural } from './model';

/**
 * Russian catalog. The localized contract rejects missing or stray keys and
 * validates message tags, plural categories, arguments, and placeholders.
 */
export const ru = defineLocalizedCatalog<'ru', EnglishCatalog>()({
  // ── App shell / navigation ──
  'nav.games': 'Игры',
  'nav.libraries': 'Библиотеки',
  'nav.settings': 'Настройки',
  'nav.operations': 'Журнал',
  'nav.gameFallback': 'Игра',
  'nav.donate': 'Поддержать',
  'shell.refresh': 'Обновить',
  'shell.updateAvailable': 'Доступно обновление',

  // ── Settings: appearance section ──
  'settings.appearance.title': 'Оформление',
  'settings.appearance.description': 'Настройте внешний вид приложения и язык.',
  'settings.appearance.theme.title': 'Тема',
  'settings.appearance.theme.description': 'Выберите цветовую тему приложения.',
  'settings.appearance.theme.triggerLabel': 'Тема',
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
  'settings.language.zhHans': '简体中文',
  'settings.language.zhHant': '繁體中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  // ── Settings: tabs ──
  'settings.tabs.general': 'Общие',
  'settings.tabs.renodx': 'RenoDX',
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
  'settings.nvidia.global.title': 'Глобальные настройки DLSS',
  'settings.nvidia.global.description':
    'Значения по умолчанию для всех игр без индивидуальных настроек — через базовый профиль NVIDIA.',
  'settings.nvidia.global.systemWide': 'Для всей системы',
  'settings.nvidia.global.adminRequired':
    'Перезапустите приложение от имени администратора, чтобы изменить эти настройки.',
  'settings.nvidia.global.familySr': 'DLSS Super Resolution',
  'settings.nvidia.global.familyFg': 'DLSS Frame Generation',
  'settings.nvidia.global.familyRr': 'DLSS Ray Reconstruction',
  'settings.nvidia.unsupported.title': 'Видеокарта NVIDIA не обнаружена',
  'settings.nvidia.unsupported.description':
    'Эти настройки доступны только при наличии поддерживаемой видеокарты NVIDIA.',

  // ── Game card ──
  'game.card.action.details': 'Подробнее',
  'game.card.action.detailsAria': 'Открыть подробности: {title}',
  'game.card.detectedLibraries': 'Найденные компоненты',
  'game.card.availableAddons': 'Доступные аддоны',
  'game.card.badge.upToDate': 'Актуально',
  'game.card.badge.updatesAvailable': 'Доступны обновления',
  'game.card.badge.updatesAvailableCount': plural('count', {
    one: 'Доступно 1 обновление',
    few: 'Доступно {count} обновления',
    many: 'Доступно {count} обновлений',
    other: 'Доступно {count} обновлений',
  }),
  'game.card.menu.ariaLabel': 'Параметры для {title}',
  'game.card.menu.favorite.add': 'Добавить в избранное',
  'game.card.menu.favorite.remove': 'Убрать из избранного',
  'game.card.menu.favorite.toggleHint': 'Переключить статус "избранное" для этой игры.',
  'game.card.menu.hidden.add': 'Скрыть игру',
  'game.card.menu.hidden.remove': 'Показать игру',
  'game.card.menu.hidden.toggleHint': 'Переключить статус видимости для этой игры.',
  'game.card.menu.removeFromCatalog': 'Удалить из каталога',
  'game.card.menu.removeFromCatalogHint': 'Убрать добавленную вручную игру из каталога.',
  'game.card.removeConfirm.title': 'Удалить «{title}» из каталога?',
  'game.card.removeConfirm.description':
    'RenderPilot безопасно отменит управляемые изменения, затем удалит карточку и связанную с ней историю. Файлы самой игры затронуты не будут.',
  'game.card.removeConfirm.action': 'Удалить из каталога',

  // ── Game cover ──
  'game.cover.alt': 'Обложка',
  'game.cover.altWithTitle': 'Обложка: {title}',
  'game.cover.menu.fetch': 'Скачать обложку',
  'game.cover.menu.fetching': 'Скачивание…',
  'game.cover.menu.fetchHint': 'Найти и скачать обложку из интернета.',
  'game.cover.menu.pick': 'Выбрать файл изображения…',
  'game.cover.menu.pickHint': 'Выбрать изображение на компьютере.',
  'game.cover.menu.clear': 'Удалить обложку',
  'game.cover.menu.clearHint': 'Вернуть стандартную обложку.',

  // ── Games dashboard summary ──
  'game.dashboard.summary': 'Сводка',
  'game.dashboard.games': plural('count', {
    one: '{count} игра',
    few: '{count} игры',
    many: '{count} игр',
    other: '{count} игр',
  }),
  'game.dashboard.updates': plural('count', {
    one: '{count} обновление',
    few: '{count} обновления',
    many: '{count} обновлений',
    other: '{count} обновлений',
  }),

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
  'error.desktopTransportFailed':
    'Служба приложения вернула некорректный ответ. Повторите действие.',
  'error.unexpectedClient': 'Произошла непредвиденная ошибка. Повторите действие.',
  'error.localeLoadFailed':
    'Не удалось загрузить выбранный язык. Интерфейс остался на предыдущем языке.',
  'error.recoveryBundlePath': 'Пакет восстановления: {path}',
  'pageLoad.loading': 'Загрузка страницы…',
  'pageLoad.error.title': 'Не удалось открыть страницу',
  'pageLoad.error.description':
    'Страница не загрузилась. Попробуйте ещё раз или вернитесь к списку игр.',
  'pageLoad.error.retry': 'Повторить',
  'pageLoad.error.backToGames': 'К играм',

  // ── Games page / catalog ──
  'games.addGame': 'Добавить игру',
  'games.addingGame': 'Добавление игры...',
  'games.chooseInstallFolder': 'Выберите папку установки игры',
  'addGame.title': 'Добавить игру',
  'addGame.cannotAddTitle': 'Не удалось добавить игру',
  'addGame.installRoot': 'Корень установки',
  'addGame.reviewTitle': 'Проверка установки игры',
  'addGame.reviewDescription': 'Подтвердите корень установки — RenderPilot добавит одну игру.',
  'addGame.selectedFolder': 'Выбранная папка',
  'addGame.recommendedFolder': 'Рекомендуемый корень установки',
  'addGame.existingRoot': 'Текущая папка игры',
  'addGame.chooseExecutable': 'Исполняемый файл игры',
  'addGame.chooseExecutablePlaceholder': 'Выберите исполняемый файл',
  'addGame.chooseAnother': 'Выбрать другую',
  'addGame.add': 'Добавить игру',
  'addGame.addSelected': 'Добавить выбранную папку',
  'addGame.correctRoot': 'Исправить путь',
  'addGame.addRecommended': 'Добавить рекомендуемый корень',
  'addGame.replaceRootTitle': 'Исправить путь к игре',
  'addGame.replaceRootDescription':
    'RenderPilot будет использовать выбранную папку вместо текущей. Файлы игры останутся без изменений.',
  'addGame.replaceExistingRoot': 'Исправить путь',
  'addGame.rootCorrection.rollbackTitle': 'Сначала нужно откатить активные замены',
  'addGame.rootCorrection.rollbackDescription': plural('count', {
    one: 'Перед заменой корня RenderPilot должен откатить активную замену 1 компонента.',
    few: 'Перед заменой корня RenderPilot должен откатить активные замены {count} компонентов.',
    many: 'Перед заменой корня RenderPilot должен откатить активные замены {count} компонентов.',
    other: 'Перед заменой корня RenderPilot должен откатить активные замены {count} компонента.',
  }),
  'addGame.rootCorrection.rollbackAndReplace': 'Откатить изменения и заменить корень',
  'addGame.rootCorrection.rollbackFailed':
    'Не удалось полностью откатить изменения компонентов. Текущий корень игры не изменён.',
  'addGame.rootCorrection.blocker.pendingRecovery':
    'Не завершено восстановление после прерванной файловой операции.',
  'addGame.rootCorrection.blocker.installedAddon':
    'Установленное дополнение относится к файлам вне выбранной папки игры.',
  'addGame.rootCorrection.blocker.nvapi':
    'Активные настройки профиля NVIDIA относятся к исполняемым файлам вне выбранной папки.',
  'addGame.rootCorrection.blocker.orphanedComponentBaseline':
    'Для сохранённого состояния отката больше не найден соответствующий компонент.',
  'addGame.rescan': 'Пересканировать игру',
  'addGame.catalogBusy':
    'Сейчас выполняется другая операция с каталогом. Завершите её и повторите действие.',
  'addGame.warning.legacyCardsConsolidated': plural('count', {
    one: 'Объединена одна подтверждённо ложная устаревшая карточка игры.',
    few: 'Объединены {count} подтверждённо ложные устаревшие карточки игры.',
    many: 'Объединено {count} подтверждённо ложных устаревших карточек игры.',
    other: 'Объединено {count} подтверждённо ложной устаревшей карточки игры.',
  }),
  'addGame.warning.legacyCardsRetained': plural('count', {
    one: 'Сохранена одна устаревшая карточка: доказательств отдельной установки недостаточно.',
    few: 'Сохранены {count} устаревшие карточки: доказательств отдельных установок недостаточно.',
    many: 'Сохранено {count} устаревших карточек: доказательств отдельных установок недостаточно.',
    other: 'Сохранено {count} устаревшей карточки: доказательств отдельных установок недостаточно.',
  }),
  'addGame.warning.recoveryBundleCreated':
    'Конфликтующее устаревшее состояние сохранено в пакете восстановления: {path}.',
  'addGame.warning.rootCorrectionHistoryArchived':
    'История каталога за пределами исправленного корня сохранена в пакете восстановления: {path}.',
  'addGame.warning.recoveryBundleFallback': 'Пакет восстановления: {path}',
  'addGame.warning.unsupportedPlatform': 'Проверка установки игры поддерживается только в Windows.',
  'addGame.warning.probeIncomplete':
    'Некоторые папки не удалось проверить. Уверенность в рекомендации снижена.',
  'addGame.warning.parentProbeIncomplete':
    'Рекомендуемую родительскую папку не удалось проверить полностью. Проверьте её перед добавлением.',
  'addGame.unavailable.multipleInstalls':
    'Выбранная папка похожа на общую библиотеку с несколькими играми. Выберите папку конкретной игры.',
  'addGame.unavailable.containsProvenInstall':
    'Внутри выбранной папки находится уже распознанная установка игры. Выберите папку этой игры, а не общий родительский каталог.',
  'addGame.unavailable.containsMultipleCatalogInstalls':
    'Внутри выбранной папки находится несколько уже распознанных игр. Выберите папку конкретной игры.',
  'addGame.unavailable.insideExistingInstall':
    'Выбрана вложенная папка уже добавленной игры. Используйте корневую папку этой игры.',
  'addGame.unavailable.noReadableExecutable':
    'В выбранной папке не найден исполняемый файл игры. Выберите папку установки, содержащую файл запуска.',
  'addGame.unavailable.rootCorrectionBlocked':
    'Существующий корень установки нельзя безопасно изменить, пока у игры есть управляемые изменения. Сначала устраните перечисленные блокирующие состояния.',
  'addGame.warning.insideExistingInstall':
    'Эта папка относится к уже добавленной игре. Используйте корень её установки.',
  'addGame.warning.narrowsExistingInstall':
    'Существующий ручной корень, вероятно, охватывает несколько папок с играми. При подтверждении сохранится та же карточка, но её корнем станет выбранная папка.',
  'addGame.warning.multipleProvenInstalls':
    'Эта папка содержит несколько подтверждённых установок игр.',
  'addGame.warning.containsProvenInstall':
    'Эта папка содержит подтверждённую установку игры. Используйте её точный корень.',
  'addGame.warning.multipleInstallsSuspected':
    'Исполняемые файлы в разных дочерних папках могут относиться к разным играм. При подтверждении эта папка всё равно будет считаться одной игрой.',
  'addGame.warning.explicitExecutableRequired':
    'Все допустимые исполняемые файлы похожи на лаунчеры или служебные программы. Выберите нужный файл вручную.',
  'addGame.warning.noReadableExecutable':
    'Эту папку нельзя добавить отдельно: в ней не найден исполняемый файл игры.',
  'addGame.warning.filesystemProbeError':
    'Часть установки не удалось проверить. Проверьте права доступа к файлам.',
  'addGame.warning.unknown':
    'При проверке игры обнаружено предупреждение, которое эта версия RenderPilot не может отобразить.',
  'games.libraryActions': 'Действия',
  'games.search': 'Поиск игр',
  'games.openFilters': 'Фильтры',
  'games.openFiltersActive': 'Фильтры (активны)',
  'games.loading': 'Загрузка...',
  'games.empty.title': 'Игры не найдены',
  'games.empty.description': 'Добавьте игру, чтобы она появилась в списке.',
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
  'settings.catalog.steamKey.show': 'Показать API-ключ',
  'settings.catalog.steamKey.hide': 'Скрыть API-ключ',
  'settings.catalog.steamKey.getKey': 'Получить API-ключ',

  // ── Settings: RenoDX ──
  'settings.renodx.vulkan.description':
    'Управление общим Vulkan-слоем ReShade для Vulkan-игр RenoDX.',
  'settings.renodx.vulkan.channel': 'Канал Vulkan-слоя',
  'settings.renodx.vulkan.channelDescription': 'Выберите канал ReShade для общего Vulkan-слоя.',
  'settings.renodx.vulkan.loadError': 'Не удалось загрузить состояние Vulkan-слоя.',
  'settings.renodx.vulkan.saveError': 'Не удалось сохранить канал Vulkan-слоя.',
  'settings.renodx.vulkan.applyError': 'Не удалось применить Vulkan-слой.',

  // ── Settings: about ──
  'settings.about.title': 'Обновления',
  'settings.about.description': 'Проверьте наличие новых версий приложения.',
  'settings.about.version.title': 'Версия приложения',
  'settings.about.version.loading': 'Определение...',
  'settings.about.checkForUpdates': 'Проверить обновления',
  'settings.about.updateInProgress': 'Обновление…',
  'settings.about.updateAvailable': 'Доступно обновление',
  'settings.about.upToDate': 'У вас установлена последняя версия',
  'settings.about.updateCheckError': 'Не удалось проверить обновления',

  'settings.about.updateDialog.title': 'Доступно обновление',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'Выпущено {date}',
  'settings.about.updateDialog.releaseNotes': 'Список изменений',
  'settings.about.updateDialog.noNotes': 'Для этого обновления нет заметок о выпуске.',
  'settings.about.updateDialog.notesTruncated': 'Список изменений был сокращён.',

  'settings.about.updateDialog.installAndRestart': 'Установить и перезапустить',
  'settings.about.updateDialog.later': 'Позже',
  'settings.about.updateDialog.close': 'Закрыть',
  'settings.about.updateDialog.retryDownload': 'Повторить загрузку',
  'settings.about.updateDialog.retryInstall': 'Повторить установку',
  'settings.about.updateDialog.restartNow': 'Перезапустить сейчас',

  'settings.about.updateDialog.downloading': 'Загрузка обновления…',
  'settings.about.updateDialog.downloadingBytes': 'Загружено {received}',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} из {total}',
  'settings.about.updateDialog.verifying': 'Проверка обновления…',
  'settings.about.updateDialog.verifyingDescription': 'Проверяется загруженный пакет.',
  'settings.about.updateDialog.installing':
    'Установка обновления… Приложение закроется; может кратко появиться установщик.',
  'settings.about.updateDialog.restarting': 'Перезапуск приложения…',

  'settings.about.updateDialog.prepareErrorTitle': 'Ошибка загрузки или проверки',
  'settings.about.updateDialog.prepareErrorDescription':
    'Не удалось загрузить или проверить обновление. Проверьте подключение и попробуйте снова.',
  'settings.about.updateDialog.installErrorTitle': 'Ошибка установки',
  'settings.about.updateDialog.installErrorDescription':
    'Не удалось установить обновление. Вы можете попробовать снова.',
  'settings.about.updateDialog.restartRequiredTitle': 'Требуется перезапуск',
  'settings.about.updateDialog.restartRequiredDescription':
    'Обновление установлено, но приложение не удалось перезапустить автоматически. Перезапустите RenderPilot вручную, чтобы завершить обновление.',

  'settings.about.updateDialog.progressAria': 'Прогресс загрузки',

  // ── Common ──
  'common.unknown': 'Неизвестно',
  'common.downloadProgress': 'Прогресс скачивания',

  // ── Game details: empty states ──
  'gameDetails.noGameSelected.title': 'Игра не выбрана',
  'gameDetails.noGameSelected.description': 'Выберите игру из списка для просмотра деталей.',

  // ── Game details: component version row ──
  'gameDetails.version.noReplacements': 'Нет альтернативных версий',
  'gameDetails.version.restoreOriginal': 'Восстановить исходный {fileName}',
  'gameDetails.version.fileCount': plural('count', {
    one: '1 файл',
    few: '{count} файла',
    many: '{count} файлов',
    other: '{count} файла',
  }),

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
  'gameDetails.streamline.mixedRange': 'Разные версии (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': 'обновлений: {updates} · отсутствует: {missing}',
  'gameDetails.streamline.restoreAllAria': 'Восстановить исходные версии',
  'gameDetails.streamline.restoreAllTooltip': 'Восстановить исходные',
  'gameDetails.updateAll.action': 'Обновить всё',
  'gameDetails.updateAll.actionCount': 'Обновить всё ({count})',
  'gameDetails.updateAll.upToDate': 'Все стабильные версии актуальны',
  'gameDetails.updateAll.partialFailure':
    'Часть обновлений не удалась ({count}). Проверьте детали и повторите.',
  'gameDetails.updateAll.tooltip': plural('count', {
    one: 'Обновить {count} компонент до последней стабильной версии',
    few: 'Обновить {count} компонента до последней стабильной версии',
    many: 'Обновить {count} компонентов до последней стабильной версии',
    other: 'Обновить {count} компонентов до последней стабильной версии',
  }),
  // ── Game details: executable selector (shared) ──
  'gameDetails.executable.title': 'Исполняемый файл игры',
  'gameDetails.developerMode.requiredTitle': 'Режим разработчика Windows выключен',
  'gameDetails.developerMode.requiredDescription':
    'Для работы Microsoft D3D12 Agility Preview требуется эта системная настройка.',
  'gameDetails.developerMode.checkTitle': 'Не удалось проверить режим разработчика',
  'gameDetails.developerMode.checkDescription':
    'RenderPilot не смог определить текущее состояние режима разработчика Windows.',
  'gameDetails.developerMode.checkUnavailable': 'Перед продолжением требуется успешная проверка.',
  'gameDetails.developerMode.enableGuidance':
    'Режим разработчика можно включить в разделе «Для разработчиков» параметров Windows.',
  'gameDetails.developerMode.previewGuidance':
    'Инструкция по включению режима разработчика доступна в документации Microsoft.',
  'gameDetails.developerMode.restartInfo':
    'В отдельных случаях изменение вступает в силу только после перезагрузки Windows.',
  'gameDetails.developerMode.stillDisabled': 'Режим разработчика по-прежнему выключен.',
  'gameDetails.developerMode.settingsOpenFailed':
    'Не удалось открыть параметры Windows. Откройте раздел «Для разработчиков» вручную.',
  'gameDetails.developerMode.documentationOpenFailed': 'Не удалось открыть документацию Microsoft.',
  'gameDetails.developerMode.openSettings': 'Открыть параметры',
  'gameDetails.developerMode.openDocumentation': 'Открыть документацию',
  'gameDetails.developerMode.checkStatus': 'Проверить статус',
  'gameDetails.developerMode.retryCheck': 'Повторить проверку',
  'gameDetails.developerMode.checkingStatus': 'Проверка…',
  'gameDetails.d3d12.status.original': 'Оригинальный EXE',
  'gameDetails.d3d12.status.patched': 'EXE пропатчен: {from} → {to}',
  'gameDetails.d3d12.status.repair': 'Требуется восстановление',
  'gameDetails.d3d12.repairGuidance':
    'Проверьте файлы игры в лаунчере и запустите сканирование повторно. RenderPilot не будет перезаписывать этот EXE.',
  'gameDetails.d3d12.action.patch': 'Патч EXE: {from} → {to}',
  'gameDetails.d3d12.action.restore': 'Восстановление EXE: {from} → {to}',
  'gameDetails.d3d12.action.repair': 'Сначала требуется восстановить EXE',
  'gameDetails.d3d12.action.blocked': 'Эту версию D3D12 нельзя применить в текущем состоянии.',
  'gameDetails.d3d12.action.planPatch': 'Будет применён патч: SDK {from} → {to}',
  'gameDetails.d3d12.action.planRestore': 'Будет восстановлен оригинальный EXE: SDK {from} → {to}',
  'gameDetails.d3d12.select.compatible': 'Совместимо с текущим EXE',
  'gameDetails.d3d12.select.changesExecutable': 'Требуется изменение EXE',
  'gameDetails.d3d12.select.unavailable': 'Недоступно',
  'gameDetails.d3d12.confirm.title': 'Подтвердите изменение EXE',
  'gameDetails.d3d12.confirm.description':
    'RenderPilot изменит экспорт D3D12SDKVersion в исполняемом файле игры.',
  'gameDetails.d3d12.confirm.updateAllDescription':
    'Для этих обновлений перечисленные EXE должны переключиться на другие SDK-линии D3D12. До подтверждения ничего не будет загружено или изменено.',
  'gameDetails.d3d12.confirm.backup': 'Путь резервной копии: {path}',
  'gameDetails.d3d12.confirm.backupWillCreate':
    'Перед изменением будет создана резервная копия оригинального EXE: {path}',
  'gameDetails.d3d12.confirm.backupExists':
    'Исходный EXE уже сохранён: {path}. Эта копия не будет перезаписана.',
  'gameDetails.d3d12.confirm.signatureWarning':
    'После изменения цифровая подпись EXE может стать недействительной, а проверка целостности может сообщить, что файл изменён. При полном откате D3D12 RenderPilot восстановит исходный EXE.',
  'gameDetails.d3d12.confirm.accept': 'Изменить',
  'gameDetails.d3d12.executableLockedTitle': 'Выбор EXE заблокирован',
  'gameDetails.d3d12.executableLocked':
    'Чтобы выбрать другой EXE, полностью откатите компонент D3D12.',
  'gameDetails.d3d12.executableRepairLocked':
    'Выполните восстановление по инструкции в карточке D3D12, затем повторите сканирование.',
  'gameDetails.executable.description':
    'Исполняемый файл игры — к нему применяется профиль NVIDIA, а RenoDX устанавливается в его папку.',
  'gameDetails.executable.triggerAria': 'Исполняемый файл игры: {fileName}',
  'gameDetails.executable.detectedGroup': 'Найденные игровые файлы',
  'gameDetails.executable.otherGroup': 'Прочее (лаунчеры, установщики, утилиты)',
  'gameDetails.executable.customBadge': 'Вручную',
  'gameDetails.executable.reset': 'Сбросить на автоопределение',
  'gameDetails.executable.tooltipAuto':
    'Исполняемый файл игры: определён автоматически. Используется профилем NVIDIA и RenoDX.',
  'gameDetails.executable.tooltipCustom':
    'Исполняемый файл игры: выбран вручную. Используется профилем NVIDIA и RenoDX.',
  // ── Game details: NVIDIA profile card ──
  'gameDetails.profile.title': 'Профиль NVIDIA',
  'gameDetails.profile.description': 'Настройте параметры драйвера NVIDIA для этой игры.',
  'gameDetails.profile.pinnedManual': 'Выбрано вручную.',
  'gameDetails.profile.autoDetected': 'Определено автоматически.',
  'gameDetails.profile.noExeDetected': 'Исполняемый файл не найден.',
  'gameDetails.profile.noExe': 'Нет файла',
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
  'libraries.catalogFallback.title': 'Каталог недоступен',
  'libraries.catalogFallback.description':
    'Показаны только локально зарегистрированные пакеты. Это неполный каталог.',
  'libraries.state.localOnly': 'Только локально',
  'libraries.state.downloaded': 'Загружено',
  'libraries.state.missing': 'Файлы отсутствуют',
  'libraries.state.corrupt': 'Файлы повреждены',
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
  'libraries.actions.downloadAll': 'Скачать последние',
  'libraries.actions.downloadAllCount': 'Скачать последние ({count})',
  'libraries.actions.downloadAllUpToDate': 'Все последние версии уже скачаны',
  'libraries.actions.downloadAllTooltip': plural('count', {
    one: 'Скачать {count} последнюю версию',
    few: 'Скачать {count} последние версии',
    many: 'Скачать {count} последних версий',
    other: 'Скачать {count} последних версий',
  }),
  'libraries.actions.downloadAllDoneToast': plural('count', {
    one: 'Скачана {count} библиотека',
    few: 'Скачано {count} библиотеки',
    many: 'Скачано {count} библиотек',
    other: 'Скачано {count} библиотек',
  }),
  'libraries.actions.downloadAllPartialToast': 'Скачано: {succeeded}, ошибок: {failed}',
  'libraries.actions.downloadAllNoneToast': 'Все последние версии уже скачаны',

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
  'filters.addons.title': 'Аддоны',

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
  'operation.duration': 'Выполнено за {duration}',
  'operation.filesUpdated.none': 'Файлы не обновлялись.',
  'operation.filesUpdated.count': plural('count', {
    one: 'Обновлён 1 файл.',
    few: 'Обновлено {count} файла.',
    many: 'Обновлено {count} файлов.',
    other: 'Обновлено {count} файлов.',
  }),
  'operation.filesRestored.none': 'Файлы не восстанавливались.',
  'operation.filesRestored.count': plural('count', {
    one: 'Восстановлен 1 файл.',
    few: 'Восстановлено {count} файла.',
    many: 'Восстановлено {count} файлов.',
    other: 'Восстановлено {count} файлов.',
  }),
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
  'notify.gameRemovedFromCatalog': 'Игра удалена из каталога.',
  'notify.removeGameFailed': 'Не удалось удалить игру из каталога.',
  'notify.applyCompleted': 'Изменения применены',
  'notify.rollbackCompleted': 'Откат выполнен',
  'notify.swapBatchFailed.title': 'Некоторые обновления не удались',
  'notify.swapBatchFailed.description': 'Не удалось обновить {failed} из {total} компонентов.',
  'notify.rollbackBatchFailed.title': 'Некоторые откаты не удались',
  'notify.rollbackBatchFailed.description':
    'Не удалось восстановить {failed} из {total} компонентов.',
  'notify.statusError': 'Ошибка',
  'notify.statusWarning': 'Предупреждение',

  // ── Library scan ──
  'scan.partialWarning': plural('count', {
    one: 'Не удалось просканировать 1 папку.',
    few: 'Не удалось просканировать {count} папки.',
    many: 'Не удалось просканировать {count} папок.',
    other: 'Не удалось просканировать {count} папок.',
  }),
  'scan.automaticFailed':
    'Не удалось выполнить автоматическое сканирование библиотек. Список игр всё равно был обновлён.',

  // ── Background cover sync ──
  'coverSync.failed': 'Не удалось синхронизировать обложки.',
  'coverSync.refreshFailed': 'Не удалось синхронизировать обложки.',
  'coverSync.failure.single': 'Не удалось загрузить обложку для «{title}»: {message}',
  'coverSync.failure.multiple': plural('count', {
    one: 'Не удалось загрузить обложки для {count} игры. Первая ошибка: {summary}',
    few: 'Не удалось загрузить обложки для {count} игр. Первая ошибка: {summary}',
    many: 'Не удалось загрузить обложки для {count} игр. Первая ошибка: {summary}',
    other: 'Не удалось загрузить обложки для {count} игры. Первая ошибка: {summary}',
  }),
  'coverSync.failure.hint': 'Проверьте источники обложек игр и настройки SteamGridDB.',

  // ── NVIDIA driver context (toasts) ──
  'nvidia.adminRequired': 'Требуются права администратора',
  'nvidia.changeSettingFailed': 'Не удалось применить настройки',
  'nvidia.revertDefaultFailed': 'Не удалось сбросить настройки',
  'nvidia.revertBaselineFailed': 'Не удалось восстановить исходные настройки',

  // ── DLSS indicator context (toasts) ──
  'indicator.changeFailed': 'Не удалось переключить индикатор DLSS',

  // ── Libraries table ──
  'libraries.column.version': 'Версия',
  'libraries.column.hash': 'Хеш',
  'libraries.column.signed': 'Подпись',
  'libraries.column.size': 'Размер',
  'libraries.column.documents': 'Документы',
  'libraries.column.actions': 'Действия',
  'libraries.documents.openForVersion': 'Открыть юридические документы для {name} {version}',
  'libraries.documents.title': 'Юридические документы',
  'libraries.documents.description': 'Применяются к {name} {version}.',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'Текст',
  'libraries.documents.open': 'Открыть',
  'libraries.documents.openFailed': 'Не удалось открыть документ',
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
  'user_message.invalid_install_root':
    'Выберите папку установки одной игры. Корень диска, сетевого ресурса и системные папки добавить нельзя.',
  'user_message.multiple_installs_detected':
    'В этой папке найдено несколько игр. Выберите папку установки одной игры.',
  'user_message.stale_install_inspection':
    'Установка изменилась во время проверки. Перед добавлением проверьте обновлённый результат.',
  'user_message.root_correction_cleanup_required':
    'Перед изменением корня игры необходимо откатить активные замены компонентов.',
  'user_message.root_correction_blocked':
    'Перед изменением корня устраните активное состояние в существующей карточке игры.',
  'user_message.managed_cleanup_ambiguous':
    'RenderPilot обнаружил пересекающиеся изменения, безопасный порядок отката которых нельзя доказать. Ничего не изменено; создан пакет восстановления.',
  'user_message.catalog_consolidation_blocked':
    'RenderPilot обнаружил конфликтующее управляемое состояние у дубликатов карточек игр. Ничего не изменено; создан пакет восстановления.',
  'user_message.game_removal_cleanup_failed':
    'RenderPilot не удалось восстановить исходные файлы игры, поэтому карточка не удалена. Проверьте файлы игры и повторите попытку.',
  'user_message.invalid_game_reference': 'Игра не найдена.',
  'user_message.invalid_component_reference': 'Компонент не найден.',
  'user_message.invalid_artifact_reference': 'Объект не найден.',
  'user_message.invalid_operation_reference': 'Действие не найдено.',
  'user_message.response_serialization_failed': 'Не удалось обработать запрос.',
  'user_message.plan_changed_rebuild': 'Задача устарела. Попробуйте снова.',
  'user_message.game_not_in_catalog': 'Игра не поддерживается.',
  'user_message.operation_not_found': 'Действие не найдено.',
  'user_message.artifact_not_found': 'Объект не найден.',
  'user_message.component_not_found': 'Компонент не найден.',
  'user_message.invalid_operation_state': 'Это действие сейчас недоступно.',
  'user_message.operation_could_not_complete': 'Не удалось выполнить действие.',
  'user_message.rollback_also_failed':
    'Действие завершилось ошибкой, и RenderPilot не смог полностью восстановить предыдущее состояние файлов. Перед повторной попыткой проверьте файлы игры.',
  'user_message.command_task_failed': 'Не удалось выполнить команду.',
  'user_message.storage_failed': 'Не удалось прочитать или записать каталог приложения.',
  'user_message.provider_failed': 'Не удалось получить данные из источника.',
  'user_message.detection_failed': 'Не удалось проанализировать файлы игры.',
  'user_message.steamgriddb_api_key_missing': 'Укажите API-ключ SteamGridDB в настройках.',
  'user_message.unsupported_cover_image_type': 'Неподдерживаемый формат изображения.',
  'user_message.cover_download_failed': 'Не удалось скачать обложку.',
  'user_message.cover_artwork_not_found': 'Обложка для этой игры не найдена.',
  'user_message.cover_file_system_error': 'Не удалось сохранить обложку на диск.',
  'user_message.stale_replacement_source':
    'Не удалось применить обновление: исходный файл был заменён или изменён вне RenderPilot. Выберите версию снова — может потребоваться загрузка.',
  'user_message.nvapi_requires_administrator':
    'Для изменения этой настройки требуются права администратора.',
  'user_message.elevation_cancelled': 'Запрос прав администратора отменён. Изменения не вносились.',
  'user_message.elevation_policy_blocked':
    'Windows заблокировала запрос прав администратора. Проверьте системную политику и повторите попытку.',
  'user_message.elevation_relaunch_failed':
    'RenderPilot не удалось перезапустить с правами администратора. Попробуйте перезапустить приложение.',
  'user_message.elevation_unsupported':
    'Перезапуск с правами администратора не поддерживается на этой платформе.',

  // ── Backend suggested actions ──
  'suggested_action.refresh_games': 'Обновите список игр и попробуйте снова.',
  'suggested_action.reload_game_details': 'Обновите информацию об игре и попробуйте снова.',
  'suggested_action.refresh_candidates': 'Обновите список и попробуйте снова.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Обновите страницу и попробуйте снова.',
  'suggested_action.retry_after_required_data': 'Подождите немного и попробуйте снова.',
  'suggested_action.inspect_logs':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.retry_or_restart':
    'Если проблема сохраняется, попробуйте перезапустить приложение.',
  'suggested_action.rebuild_operation_plan': 'Пожалуйста, начните действие заново.',
  'suggested_action.refresh_or_scan_game_folder': 'Обновите список или отсканируйте папку заново.',
  'suggested_action.relaunch_as_administrator':
    'Перезапустите приложение от имени администратора и попробуйте снова.',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description': 'Добавьте HDR и тон-маппинг в игру через ReShade-аддон RenoDX.',
  'gameDetails.renodx.loading': 'Проверка доступности…',
  'gameDetails.renodx.installError': 'Не удалось установить RenoDX',
  'gameDetails.renodx.uninstallError': 'Не удалось удалить RenoDX',
  'gameDetails.renodx.switchError': 'Не удалось переключить канал ReShade',
  'gameDetails.renodx.unsupported': 'Для этой игры нет профиля RenoDX.',
  'gameDetails.renodx.incompatible': 'RenoDX нельзя установить: {reason}.',
  'gameDetails.renodx.status.label': 'Статус',
  'gameDetails.renodx.statusInstalled': 'Установлен',
  'gameDetails.renodx.actionInstall': 'Установить',
  'gameDetails.renodx.actionUninstall': 'Удалить RenoDX',
  'gameDetails.renodx.actionRepair': 'Восстановить',
  'gameDetails.renodx.uninstallConfirmTitle': 'Удалить RenoDX из этой игры?',
  'gameDetails.renodx.uninstallConfirmBody':
    'Будет удалён аддон RenoDX и восстановлены только файлы ReShade, изменённые во время настройки RenoDX.',
  'gameDetails.renodx.uninstallConfirmAction': 'Удалить',
  'gameDetails.renodx.installing': 'Установка…',
  'gameDetails.renodx.confirmTitle': 'Установить RenoDX несмотря на риск из-за античита?',
  'gameDetails.renodx.cancel': 'Отмена',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.removeError': 'Не удалось удалить общий Vulkan-слой ReShade.',
  'gameDetails.renodx.vulkanLayer.title': 'Общий Vulkan-слой',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': 'Удалить общий Vulkan-слой?',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    'Удаление общего Vulkan-слоя ReShade влияет на все Vulkan-игры RenoDX. Продолжить?',
  'gameDetails.renodx.vulkanLayer.openSettings': 'Открыть настройки RenoDX',
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    'Обнаружен существующий Vulkan-слой; в этой версии только для чтения',
  'gameDetails.renodx.vulkanLayer.state.not_installed': 'Не установлен',
  'gameDetails.renodx.vulkanLayer.state.installed': 'Установлен',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Отключён в реестре',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': 'Только для чтения',
  'gameDetails.renodx.vulkanLayer.state.conflict': 'Конфликт',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': 'Требует ремонта',
  'gameDetails.renodx.vulkanLayer.state.unsupported': 'Не поддерживается',
  'gameDetails.renodx.vulkanLayer.action.install': 'Установить',
  'gameDetails.renodx.vulkanLayer.action.update': 'Обновить',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'Сменить канал',
  'gameDetails.renodx.vulkanLayer.action.repair': 'Починить слой',
  'gameDetails.renodx.vulkanLayer.action.remove': 'Удалить',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    'Обнаружен существующий Vulkan-слой.',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    'Зарегистрировано несколько манифестов слоёв ReShade.',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'Видимость загрузчика неоднозначна.',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll': 'Отсутствует DLL слоя.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'DLL слоя не удалось прочитать (нет доступа или файл заблокирован).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'Отсутствует манифест слоя.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'Файлы слоя есть, но запись в загрузчике Vulkan отсутствует.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'Запись в реестре загрузчика отключена.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'Архитектура слоя не поддерживается.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'Слой зарегистрирован в HKCU и может не загружаться для игр, запущенных с повышенными правами.',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'Не удалось разобрать манифест слоя.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    'Требуемую область реестра нельзя перезаписать.',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'Операционная система отклонила требуемую операцию.',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'Проверка бэкенда не прошла; слой требует проверки.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'Хэш DLL слоя не совпадает с ожидаемой версией.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'DLL слоя отсутствует; используется advisory-запись из базы данных.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': 'неподдерживаемый графический API',
  'gameDetails.renodx.reason.api_not_allowed': 'графический API не разрешён для этой игры',
  'gameDetails.renodx.reason.arch_unknown': 'неизвестная разрядность исполняемого файла',
  'gameDetails.otherTab': 'Другое',
  'gameDetails.renodx.unavailable': 'RenoDX сейчас недоступен.',
  'renodx.generic.universal': 'Универсальный RenoDX',
  'renodx.generic.unity': 'Универсальный RenoDX (Unity)',
  'gameDetails.renodx.generic.profileTooltip': 'Используется общий профиль движка.',
  'renodx.phase.finalizing': 'Завершение…',
  'luma.phase.finalizing': 'Завершение…',
  'gameDetails.renodx.confidenceLabel': 'Совместимость RenoDX',
  'gameDetails.renodx.confidenceVerified': 'Работает',
  'gameDetails.renodx.confidenceExperimental': 'В работе',
  'gameDetails.renodx.confidenceUntested': 'Не проверено',
  'gameDetails.renodx.external':
    'Это дополнение RenoDX распространяется отдельно и должно быть загружено вручную.',
  'gameDetails.renodx.actionOpenExternal': 'Открыть страницу загрузки',
  'gameDetails.renodx.external.installFromFile': 'Установить из файла',
  'gameDetails.renodx.external.dropHint':
    'Скачайте дополнение, затем перетащите его сюда или выберите файл.',
  'gameDetails.renodx.external.invalidFile':
    'Этот файл не является дополнением RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.title': 'Ручная установка',
  'gameDetails.renodx.fileInstall.chooseFile': 'Выбрать файл аддона…',
  'gameDetails.renodx.fileInstall.chooseAnother': 'Выбрать другой файл',
  'gameDetails.renodx.fileInstall.expected': 'Ожидаемый аддон: {name}',
  'gameDetails.renodx.fileInstall.confirm': 'Установить {fileName}?',
  'gameDetails.renodx.fileInstall.errorExtension':
    'Это не файл аддона RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.errorArch':
    'Этот аддон {addon}, а игра {game}. Скачайте подходящий аддон.',
  'gameDetails.renodx.fileInstall.warnName':
    'Это не похоже на ожидаемый аддон ({expected}). Устанавливайте, только если уверены.',
  'gameDetails.renodx.nativeHdr': 'Эта игра уже поддерживает нативный HDR — RenoDX не требуется.',
  'gameDetails.renodx.blacklisted': 'RenoDX не рекомендуется для этой игры.',
  'gameDetails.renodx.updatesNotTracked': 'Обновления не отслеживаются',
  'gameDetails.renodx.channel.label': 'Канал ReShade-хоста',
  'gameDetails.renodx.channel.hostLabel': 'ReShade-хост',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'Версия неизвестна',
  'gameDetails.renodx.host.addons.none': 'аддоны не поддерживаются',
  'gameDetails.renodx.host.addons.unknown': 'поддержка аддонов неизвестна',
  'gameDetails.renodx.host.action.update_host': 'доступно обновление',
  'gameDetails.renodx.host.action.repair_host': 'Восстановить ReShade для поддержки аддонов RenoDX',
  'gameDetails.renodx.host.customBuild':
    'Кастомная сборка (например, GShade) — вы обновляете её сами',
  'gameDetails.renodx.host.conflictMultiple':
    'Найдено несколько хостов ReShade — проверьте активный слот',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'Слот ReShade, который использует игра, занят другим файлом, или ReShade установлен в другом слоте — решите это перед установкой.',
  'gameDetails.renodx.actionUpdate': 'Обновить',
  'gameDetails.renodx.updating': 'Обновление…',
  'gameDetails.renodx.updateError': 'Не удалось обновить RenoDX',
  'gameDetails.renodx.actionInstallDlssFix': 'Установить',
  'gameDetails.renodx.actionRemoveDlssFix': 'Удалить',
  'gameDetails.renodx.dlssFixInstallError': 'Ошибка установки DLSS-Fix',
  'gameDetails.renodx.dlssFixRemoveError': 'Ошибка удаления DLSS-Fix',
  'gameDetails.renodx.fresh.label': 'Версия',
  'gameDetails.renodx.fresh.current': 'Последняя',
  'gameDetails.renodx.fresh.available': 'Доступно обновление',
  'gameDetails.renodx.fresh.channelMismatch': 'Доступна смена канала',
  'gameDetails.renodx.fresh.validationRequired': 'Требуется проверка',
  'gameDetails.renodx.fresh.unknown': 'Не удалось проверить',
  'gameDetails.renodx.fresh.checking': 'Проверка…',
  'gameDetails.renodx.addonDated': 'Аддон от {date}',
  'gameDetails.renodx.installedOn': 'Установлено {date}',
  'gameDetails.renodx.lastChecked': 'Проверено {time}',
  'gameDetails.renodx.lastCheckedNever': 'Ещё не проверялось',
  'gameDetails.renodx.actionCheckUpdates': 'Проверить обновления',
  'gameDetails.renodx.component.reshade': 'Хост ReShade',
  'gameDetails.renodx.component.addon': 'Аддон RenoDX',
  'gameDetails.renodx.component.addonDesc': 'HDR-аддон для этой игры',
  'gameDetails.renodx.component.addonDisabled': 'Установлен, но отключён в ReShade.ini',
  'gameDetails.renodx.component.addonFileInstall':
    'Установлено из файла — обновления не отслеживаются',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'Убирает мерцание при DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixOffer':
    'Доступно — убирает мерцание при DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixHint':
    'Это общее исправление для ReShade, не специфичное для RenoDX. Оно заставляет ReShade рисовать по нативным кадрам игры, а не по кадрам Frame Generation, и скрывает DLSS-апскейлинг от ReShade, если игра корректно реализует Streamline.',
  'gameDetails.renodx.attribution': 'RenoDX от clshortfuse.',
  'gameDetails.renodx.attributionLink': 'Открыть проект',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'Античит не обнаружен — установка безопасна.',
  'gameDetails.addon.riskWarn': 'Обнаружен античит — установка может привести к бану.',
  'addon.risk.sp_safe':
    'Известные сигнатуры античита не обнаружены — установка {addonName}, скорее всего, безопасна.',
  'addon.risk.anticheat_detected':
    'Обнаружены сигнатуры античита — установка {addonName} может привести к бану.',
  'gameDetails.addon.confirmAccept': 'Всё равно установить',
  'gameDetails.addon.confirmBody':
    'В игре используется античит. ReShade-аддон может его активировать и привести к бану. Продолжайте на свой риск.',
  'gameDetails.addon.fullAddonWarning':
    'Полная поддержка аддонов ReShade может быть небезопасна для мультиплеера или игр с античитом.',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    'Для этой игры установлен {installedAddon} — удалите его перед установкой {blockedAddon}.',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    'На диске найдены файлы {installedAddon} для этой игры — удалите их перед установкой {blockedAddon}.',
  'addon.availability.loadFailed': 'Не удалось проверить',
  'addon.availability.retry': 'Повторить',
  'addon.availability.checking': 'Проверка…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description': 'Возможности Luma для этой игры указаны ниже.',
  'gameDetails.luma.loading': 'Проверка доступности…',
  'gameDetails.luma.installError': 'Не удалось установить Luma',
  'gameDetails.luma.uninstallError': 'Не удалось удалить Luma',
  'gameDetails.luma.updateError': 'Не удалось обновить Luma',
  'gameDetails.luma.repairError': 'Не удалось восстановить Luma',
  'gameDetails.luma.unsupported': 'Для этой игры нет профиля Luma.',
  'gameDetails.luma.incompatible': 'Luma нельзя установить: {reason}.',
  'gameDetails.luma.blacklisted': 'Luma не рекомендуется для этой игры.',
  'gameDetails.luma.unavailable': 'Luma сейчас недоступна.',
  'gameDetails.luma.unmanagedPresent':
    'На диске найдена установка Luma без отслеживаемой записи. Удалите её вручную, затем переустановите.',
  'gameDetails.luma.installTornWarning':
    'Предыдущая установка завершилась некорректно. Повторная установка очистит и восстановит её.',
  'gameDetails.luma.installTornWarningInstalled':
    'Последняя операция завершилась некорректно. Используйте «Восстановить» (или «Обновить», если доступно), чтобы завершить согласование установки.',
  'gameDetails.luma.status.label': 'Статус',
  'gameDetails.luma.statusInstalled': 'Установлена',
  'gameDetails.luma.actionInstall': 'Установить',
  'gameDetails.luma.installing': 'Установка…',
  'gameDetails.luma.actionUninstall': 'Удалить Luma',
  'gameDetails.luma.actionRepair': 'Восстановить',
  'gameDetails.luma.actionUpdate': 'Обновить',
  'gameDetails.luma.updating': 'Обновление…',
  'gameDetails.luma.actionCheckUpdates': 'Проверить обновления',
  'gameDetails.luma.uninstallConfirmTitle': 'Удалить Luma из этой игры?',
  'gameDetails.luma.uninstallConfirmBody':
    'Luma будет удалена. Если DLSS DLL принадлежит Luma, её Library Swap будет отменён с восстановлением точного состояния до Luma. Переиспользованные DLL и независимые swap-операции останутся без изменений.',
  'gameDetails.luma.uninstallConfirmAction': 'Удалить',
  'gameDetails.luma.confirmTitle': 'Установить Luma несмотря на риск из-за античита?',
  'gameDetails.luma.vcredistWarning':
    'На системе может отсутствовать актуальный Visual C++ Redistributable. Если Luma не загружается, установите redistributable.',
  'gameDetails.luma.vcredistLink': 'Скачать redistributable',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot установит и настроит dgVoodoo2 {version} для этого профиля Luma.',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Совместимость Luma',
  'gameDetails.luma.confidenceVerified': 'Работает',
  'gameDetails.luma.confidenceExperimental': 'В работе',
  'gameDetails.luma.confidenceUntested': 'Не проверено',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': 'Используется общий профиль движка.',
  'gameDetails.luma.features.title': 'Возможности',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': 'Поддерживается',
  'gameDetails.luma.features.unsupported': 'Не поддерживается',
  'gameDetails.luma.features.experimental': 'Экспериментально',
  'gameDetails.luma.features.unknown': 'Неизвестно',
  'gameDetails.luma.guidance.gameSetting': 'Настройка в игре',
  'gameDetails.luma.guidance.engineIni': 'Ручное изменение INI',
  'gameDetails.luma.guidance.launchArgument': 'Аргумент запуска',
  'gameDetails.luma.guidance.warning': 'Важно',
  'gameDetails.luma.guidance.compatibility': 'Примечание о совместимости',
  'gameDetails.luma.guidance.externalTool': 'Сторонний инструмент',
  'gameDetails.luma.guidance.copy': 'Копировать',
  'gameDetails.luma.guidance.copied': 'Скопировано',
  'gameDetails.luma.guidance.copyFailed': 'Не удалось скопировать',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': 'неподдерживаемый графический API',
  'gameDetails.luma.reason.api_not_allowed': 'графический API не разрешён для этой игры',
  'gameDetails.luma.reason.arch_unknown': 'неизвестная разрядность исполняемого файла',
  'gameDetails.luma.reason.arch_mismatch': 'разрядность исполняемого файла не подходит для аддона',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'Версия неизвестна',
  'gameDetails.luma.host.addons.none': 'аддоны не поддерживаются',
  'gameDetails.luma.host.addons.unknown': 'поддержка аддонов неизвестна',
  'gameDetails.luma.host.action.update_host': 'доступно обновление',
  'gameDetails.luma.host.action.repair_host': 'Восстановить ReShade для поддержки аддона Luma',
  'gameDetails.luma.host.customBuild':
    'Кастомная сборка (например, GShade) — вы обновляете её сами',
  'gameDetails.luma.host.conflictMultiple':
    'Найдено несколько хостов ReShade — проверьте активный слот',
  'gameDetails.luma.host.conflictBlocksInstall':
    'Слот ReShade, который использует игра, занят другим файлом, или ReShade установлен в другом слоте — решите это перед установкой.',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'Версия',
  'gameDetails.luma.fresh.current': 'Последняя',
  'gameDetails.luma.fresh.available': 'Доступно обновление',
  'gameDetails.luma.fresh.channelMismatch': 'Доступна смена канала',
  'gameDetails.luma.fresh.validationRequired': 'Требуется проверка',
  'gameDetails.luma.fresh.unknown': 'Не удалось проверить',
  'gameDetails.luma.fresh.checking': 'Проверка…',
  'gameDetails.luma.updatesNotTracked': 'Обновления не отслеживаются',
  'gameDetails.luma.addonDated': 'Аддон от {date}',
  'gameDetails.luma.installedOn': 'Установлено {date}',
  'gameDetails.luma.lastChecked': 'Проверено {time}',
  'gameDetails.luma.lastCheckedNever': 'Ещё не проверялось',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'Хост ReShade',
  'gameDetails.luma.component.addon': 'Аддон Luma',
  'gameDetails.luma.component.addonDesc': 'Возможности Luma для этой игры',
  'gameDetails.luma.component.dgvoodoo': 'Wrapper dgVoodoo2',
  'gameDetails.luma.component.dgvoodooDesc': 'Управляемый D3D9-мост, версия {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'Если запускаете игру через Steam, добавьте их там: правой кнопкой по игре → Свойства → Общие → Параметры запуска.',
  'gameDetails.luma.launchArgs.instructions.gog':
    'Если запускаете игру через GOG Galaxy, добавьте их там: настройки игры → Управление установкой → Настроить.',
  'gameDetails.luma.launchArgs.instructions.epic':
    'Если запускаете игру через Epic Games Launcher, добавьте их там: правой кнопкой по игре → Управление → Дополнительные аргументы командной строки.',
  'gameDetails.luma.launchArgs.instructions.ea':
    'Если запускаете игру через EA app, добавьте их там: выберите игру → Управление → Просмотреть свойства → Расширенные параметры запуска.',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'Если запускаете игру через Ubisoft Connect, добавьте их там: выберите игру → Свойства → Добавить аргументы запуска.',
  'gameDetails.luma.launchArgs.instructions.other':
    'Используйте способ запуска, которым игра действительно запускается. Добавьте аргументы в лаунчер, цель ярлыка, bat-файл или другой загрузчик.',
  'gameDetails.luma.launchArgs.title': 'Требуются параметры запуска',
  'gameDetails.luma.launchArgs.dx11Title': 'Для этого профиля Luma требуется DirectX 11',
  'gameDetails.luma.launchArgs.copyStep': 'Скопируйте требуемые параметры запуска:',
  'gameDetails.luma.launchArgs.copy': 'Скопировать аргументы',
  'gameDetails.luma.launchArgs.copied': 'Скопировано',
  'gameDetails.luma.launchArgs.copyFailed': 'Не удалось скопировать аргументы запуска',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework от Filoppi.',
  'gameDetails.luma.attributionLink': 'Открыть проект',
});
