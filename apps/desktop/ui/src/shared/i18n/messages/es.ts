import type { MessageKey } from './en';
import type { MessageValue } from './types';

export const es: Record<MessageKey, MessageValue> = {
  'nav.games': 'Juegos',
  'nav.libraries': 'Bibliotecas',
  'nav.settings': 'Ajustes',
  'nav.operations': 'Diario',
  'nav.gameFallback': 'Juego',
  'nav.donate': 'Donar',
  'shell.refresh': 'Actualizar',
  'shell.updateAvailable': 'Actualización disponible',

  'settings.appearance.title': 'Apariencia',
  'settings.appearance.description': 'Personaliza el aspecto de la aplicación y el idioma.',
  'settings.appearance.theme.title': 'Tema',
  'settings.appearance.theme.description': 'Elige un tema de color para la aplicación.',
  'settings.appearance.theme.triggerLabel': 'Tema',
  'settings.appearance.language.title': 'Idioma',
  'settings.appearance.language.description': 'Selecciona el idioma de la interfaz.',
  'settings.appearance.language.triggerLabel': 'Idioma',
  'settings.appearance.language.placeholder': 'Seleccionar idioma',

  'settings.theme.system': 'Sistema',
  'settings.theme.dark': 'Oscuro',
  'settings.theme.light': 'Claro',

  'settings.language.system': 'Valor por defecto del sistema',
  'settings.language.en': 'English',
  'settings.language.ru': 'Русский',
  'settings.language.es': 'Español',
  'settings.language.zh': '中文',
  'settings.language.fr': 'Français',
  'settings.language.de': 'Deutsch',
  'settings.language.ja': '日本語',

  'settings.tabs.general': 'General',
  'settings.tabs.renodx': 'RenoDX',
  'settings.tabs.catalog': 'Catálogo',
  'settings.tabs.nvidia': 'NVIDIA',

  // ── Settings: NVIDIA ──
  'settings.nvidia.indicator.title': 'Indicador DLSS',
  'settings.nvidia.indicator.description':
    'Muestra una superposición con la versión y configuración activa de DLSS durante el juego.',
  'settings.nvidia.indicator.systemWide': 'En todo el sistema',
  'settings.nvidia.indicator.adminRequired':
    'Reinicia la aplicación como administrador para cambiar esta configuración.',
  'settings.nvidia.indicator.overlayTitle': 'Superposición en pantalla',
  'settings.nvidia.indicator.overlayDescription': 'Se aplica a todos los juegos en esta PC.',
  'settings.nvidia.indicator.toggleAria': 'Alternar indicador DLSS',
  'settings.nvidia.global.title': 'Ajustes globales de DLSS',
  'settings.nvidia.global.description':
    'Valores predeterminados aplicados a cada juego sin anulación específica, mediante el perfil base de NVIDIA.',
  'settings.nvidia.global.systemWide': 'En todo el sistema',
  'settings.nvidia.global.adminRequired':
    'Reinicia la aplicación como administrador para cambiar estos ajustes.',
  'settings.nvidia.global.familySr': 'DLSS Super Resolution',
  'settings.nvidia.global.familyFg': 'DLSS Frame Generation',
  'settings.nvidia.global.familyRr': 'DLSS Ray Reconstruction',
  'settings.nvidia.unsupported.title': 'No se detectó ninguna GPU NVIDIA',
  'settings.nvidia.unsupported.description':
    'Estos ajustes requieren una tarjeta gráfica NVIDIA compatible.',

  'game.card.action.details': 'Detalles',
  'game.card.action.detailsAria': 'Abrir detalles de {title}',
  'game.card.detectedLibraries': 'Componentes detectados',
  'game.card.availableAddons': 'Complementos disponibles',
  'game.card.badge.upToDate': 'Actualizado',
  'game.card.badge.updatesAvailable': 'Actualizaciones disponibles',
  'game.card.badge.updatesAvailableCount': {
    one: '1 actualización disponible',
    other: '{count} actualizaciones disponibles',
  },
  'game.card.menu.ariaLabel': 'Opciones para {title}',
  'game.card.menu.favorite.add': 'Añadir a favoritos',
  'game.card.menu.favorite.remove': 'Eliminar de favoritos',
  'game.card.menu.favorite.toggleHint': 'Alternar el estado de favorito para este juego.',
  'game.card.menu.hidden.add': 'Ocultar juego',
  'game.card.menu.hidden.remove': 'Mostrar juego',
  'game.card.menu.hidden.toggleHint': 'Alternar el estado de oculto para este juego.',

  'game.cover.alt': 'Carátula',
  'game.cover.altWithTitle': 'Carátula: {title}',
  'game.cover.menu.fetch': 'Descargar carátula',
  'game.cover.menu.fetching': 'Descargando…',
  'game.cover.menu.fetchHint': 'Buscar una carátula en línea.',
  'game.cover.menu.pick': 'Elegir archivo de imagen…',
  'game.cover.menu.pickHint': 'Selecciona una imagen local para usar como carátula.',
  'game.cover.menu.clear': 'Eliminar carátula',
  'game.cover.menu.clearHint': 'Restaurar la carátula predeterminada.',

  'game.dashboard.summary': 'Resumen',
  'game.dashboard.games': { one: '{count} juego', other: '{count} juegos' },
  'game.dashboard.updates': { one: '{count} actualización', other: '{count} actualizaciones' },

  'elevation.title': 'Se requieren privilegios de administrador',
  'elevation.description':
    'Algunas configuraciones no se pueden cambiar sin derechos de administrador.',
  'elevation.relaunch': 'Reiniciar como administrador',
  'elevation.relaunchFailed': 'No se pudo reiniciar como administrador',
  'elevation.dismiss': 'Descartar',
  'error.boundary.title': 'Algo salió mal',
  'error.boundary.description':
    'Esta pantalla encontró un error inesperado. Vuelve a intentarlo o cambia a otra sección.',
  'error.boundary.reset': 'Reintentar',
  'pageLoad.loading': 'Cargando página…',
  'pageLoad.error.title': 'No se pudo abrir esta página',
  'pageLoad.error.description':
    'No se pudo cargar la página. Vuelve a intentarlo o regresa a Juegos.',
  'pageLoad.error.retry': 'Reintentar',
  'pageLoad.error.backToGames': 'Volver a Juegos',

  'games.scanFolder': 'Escanear carpeta',
  'games.scanning': 'Escaneando...',
  'games.libraryActions': 'Acciones',
  'games.search': 'Buscar juegos',
  'games.openFilters': 'Filtros',
  'games.openFiltersActive': 'Filtros (activos)',
  'games.loading': 'Cargando...',
  'games.empty.title': 'No se encontraron juegos',
  'games.empty.description': 'Escanea una carpeta para añadir juegos al panel.',
  'games.filterEmpty.title': 'No se encontraron coincidencias',
  'games.filterEmpty.description': 'Intenta cambiar tu búsqueda o filtros.',
  'games.filterEmpty.reset': 'Restablecer filtros',

  'settings.catalog.title': 'Fuentes de carátulas',
  'settings.catalog.description': 'Selecciona fuentes en línea para descargar carátulas de juegos.',
  'settings.catalog.steamKey.srLabel': 'Clave API de SteamGridDB',
  'settings.catalog.steamKey.placeholder': 'Clave API',
  'settings.catalog.steamKey.loading': 'Cargando…',
  'settings.catalog.steamKey.save': 'Guardar',
  'settings.catalog.steamKey.saved': 'Guardado',
  'settings.catalog.steamKey.cleared': 'Eliminado',
  'settings.catalog.steamKey.readError': 'Error al leer la configuración.',
  'settings.catalog.steamKey.saveError': 'Error al guardar la configuración.',
  'settings.catalog.steamKey.show': 'Mostrar clave de API',
  'settings.catalog.steamKey.hide': 'Ocultar clave de API',
  'settings.catalog.steamKey.getKey': 'Obtener una clave de API',

  'settings.renodx.vulkan.description':
    'Gestiona la capa Vulkan compartida de ReShade para juegos Vulkan con RenoDX.',
  'settings.renodx.vulkan.channel': 'Canal de la capa Vulkan',
  'settings.renodx.vulkan.channelDescription':
    'Elige qué canal de ReShade usará la capa Vulkan compartida.',
  'settings.renodx.vulkan.loadError': 'No se pudo cargar el estado de la capa Vulkan.',
  'settings.renodx.vulkan.saveError': 'No se pudo guardar el canal de la capa Vulkan.',
  'settings.renodx.vulkan.applyError': 'No se pudo aplicar la capa Vulkan.',

  'common.unknown': 'Desconocido',
  'common.downloadProgress': 'Progreso de descarga',

  'gameDetails.noGameSelected.title': 'Ningún juego seleccionado',
  'gameDetails.noGameSelected.description': 'Selecciona un juego del panel para ver sus detalles.',

  'gameDetails.version.noReplacements': 'Sin versiones alternativas',
  'gameDetails.version.restoreOriginal': 'Restaurar {fileName} original',
  'gameDetails.version.fileCount': { one: '1 archivo', other: '{count} archivos' },

  'gameDetails.vendor.description': 'Cambiar la versión del componente.',

  'gameDetails.dlss.description': 'Cambiar la versión de DLSS o anular su configuración.',
  'gameDetails.dlss.descriptionSwapOnly': 'Cambiar la versión de DLSS.',
  'gameDetails.dlss.libraryFileLabel': 'Versión del archivo',
  'gameDetails.dlss.driverOverridesLabel': 'Anulaciones de perfil de NVIDIA',
  'gameDetails.dlss.adminRequired':
    'Reinicia la aplicación como administrador para cambiar esta configuración.',

  'gameDetails.streamline.description': 'Administrar complementos de Streamline.',
  'gameDetails.streamline.versionTitle': 'Versión global de Streamline',
  'gameDetails.streamline.versionDescription': 'Aplica la misma versión a todos los complementos.',
  'gameDetails.streamline.noOtherVersions': 'Sin otras versiones',
  'gameDetails.streamline.mixed': 'Versiones mixtas',
  'gameDetails.streamline.mixedRange': 'Versiones mixtas (v{min} – v{max})',
  'gameDetails.streamline.updatesSummary': '{updates} actualizaciones · {missing} faltantes',
  'gameDetails.streamline.restoreAllAria': 'Restaurar todos los complementos a su estado original',
  'gameDetails.streamline.restoreAllTooltip': 'Restaurar todo a su estado original',
  'gameDetails.updateAll.action': 'Actualizar todo',
  'gameDetails.updateAll.actionCount': 'Actualizar todo ({count})',
  'gameDetails.updateAll.upToDate': 'Todas las versiones estables están actualizadas',
  'gameDetails.updateAll.partialFailure':
    'Algunas actualizaciones fallaron ({count}). Revisa los detalles e inténtalo de nuevo.',
  'gameDetails.updateAll.tooltip': {
    one: 'Actualizar 1 componente a su última versión estable',
    other: 'Actualizar {count} componentes a sus últimas versiones estables',
  },
  'gameDetails.executable.title': 'Ejecutable del juego',
  'gameDetails.d3d12.status.original': 'EXE original',
  'gameDetails.d3d12.status.patched': 'EXE parcheado: {from} → {to}',
  'gameDetails.d3d12.status.repair': 'Reparación necesaria',
  'gameDetails.d3d12.repairGuidance':
    'Verifica los archivos del juego y vuelve a escanear. RenderPilot no sobrescribirá este EXE.',
  'gameDetails.d3d12.action.patch': 'Parchear EXE: {from} → {to}',
  'gameDetails.d3d12.action.restore': 'Restaurar EXE: {from} → {to}',
  'gameDetails.d3d12.action.repair': 'Primero hay que reparar el EXE',
  'gameDetails.d3d12.action.blocked':
    'Esta versión de D3D12 no se puede aplicar en el estado actual.',
  'gameDetails.d3d12.action.planPatch': 'Se aplicará un parche: SDK {from} → {to}',
  'gameDetails.d3d12.action.planRestore': 'Se restaurará el EXE original: SDK {from} → {to}',
  'gameDetails.d3d12.select.compatible': 'Compatible con el EXE actual',
  'gameDetails.d3d12.select.changesExecutable': 'Requiere cambiar el EXE',
  'gameDetails.d3d12.select.unavailable': 'No disponible',
  'gameDetails.d3d12.confirm.title': 'Confirmar cambio del EXE',
  'gameDetails.d3d12.confirm.description':
    'RenderPilot cambiará la exportación D3D12SDKVersion del ejecutable.',
  'gameDetails.d3d12.confirm.updateAllDescription':
    'Estas actualizaciones requieren cambiar la línea SDK de D3D12 de los ejecutables indicados. No se descargará ni cambiará nada hasta que confirmes.',
  'gameDetails.d3d12.confirm.backup': 'Ruta de la copia: {path}',
  'gameDetails.d3d12.confirm.backupWillCreate':
    'Antes del cambio se creará una copia de seguridad del EXE original en: {path}',
  'gameDetails.d3d12.confirm.backupExists':
    'El EXE original ya está guardado en: {path}. Esta copia no se sobrescribirá.',
  'gameDetails.d3d12.confirm.signatureWarning':
    'Después del cambio, la firma digital del EXE puede considerarse no válida y las comprobaciones de integridad pueden detectar que el archivo fue modificado. Al revertir D3D12 por completo, RenderPilot restaurará el EXE original.',
  'gameDetails.d3d12.confirm.accept': 'Cambiar',
  'gameDetails.d3d12.executableLockedTitle': 'Selección de EXE bloqueada',
  'gameDetails.d3d12.executableLocked':
    'Para seleccionar otro EXE, revierte por completo el componente D3D12.',
  'gameDetails.d3d12.executableRepairLocked':
    'Sigue las instrucciones de recuperación de la tarjeta D3D12 y vuelve a analizar el juego.',
  'gameDetails.executable.description':
    'El ejecutable del juego: el perfil de NVIDIA se aplica a él y RenoDX se instala en su carpeta.',
  'gameDetails.executable.triggerAria': 'Ejecutable del juego: {fileName}',
  'gameDetails.executable.detectedGroup': 'Ejecutables del juego detectados',
  'gameDetails.executable.otherGroup': 'Otros (lanzadores, instaladores, herramientas)',
  'gameDetails.executable.customBadge': 'Manual',
  'gameDetails.executable.reset': 'Restablecer a detección automática',
  'gameDetails.executable.tooltipAuto':
    'Ejecutable del juego: detectado automáticamente. Usado por el perfil de NVIDIA y RenoDX.',
  'gameDetails.executable.tooltipCustom':
    'Ejecutable del juego: seleccionado manualmente. Usado por el perfil de NVIDIA y RenoDX.',
  'gameDetails.profile.title': 'Perfil de NVIDIA',
  'gameDetails.profile.description':
    'Configura los ajustes del controlador NVIDIA para este juego.',
  'gameDetails.profile.pinnedManual': 'Seleccionado manualmente.',
  'gameDetails.profile.autoDetected': 'Detectado automáticamente.',
  'gameDetails.profile.noExeDetected': 'No se encontró ningún archivo ejecutable para este juego.',
  'gameDetails.profile.noExe': 'Sin ejecutable',
  'gameDetails.profile.noProfile': 'No se encontró el perfil de NVIDIA.',

  'gameDetails.nvapi.requiresDriver': 'requiere controlador {version}+',
  'gameDetails.nvapi.unavailable': 'no disponible',
  'gameDetails.nvapi.resetDefault': 'Restablecer por defecto',
  'gameDetails.nvapi.alreadyDefault': 'Ya es por defecto',
  'gameDetails.nvapi.restoreBaselineAria': 'Restaurar valor inicial',
  'gameDetails.nvapi.restoreBaseline': 'Restaurar valor inicial',
  'gameDetails.nvapi.alreadyBaseline': 'Ya en el valor inicial',
  'gameDetails.nvapi.noBaseline': 'Ningún valor inicial guardado',

  'gameDetails.nvapi.warning.noDll':
    'No se detectó ningún archivo DLL de DLSS en el directorio de instalación.',
  'gameDetails.nvapi.warning.noManifest': 'El manifiesto no tiene datos para esta versión de DLL.',
  'gameDetails.nvapi.warning.noExecutable':
    'No se encontró ningún archivo ejecutable para este juego.',
  'gameDetails.nvapi.warning.nvapiUnavailable': 'NVAPI no está disponible.',
  'gameDetails.nvapi.warning.nvapiInitFailed': 'Error al inicializar NVAPI.',
  'gameDetails.nvapi.warning.drsFailed': 'No se pudo crear la sesión DRS.',

  'operations.title': 'Historial',
  'operations.subtitleGame': 'Actividad de {title}',
  'operations.loading': 'Cargando...',
  'operations.empty': 'Aún no hay historial',
  'operations.gameName': 'Juego',
  'operations.date': 'Fecha',
  'operations.status': 'Estado',
  'operations.action': 'Acción',
  'operations.libraryType': 'Tipo de biblioteca',
  'operations.version': 'Versión',

  'libraries.error': 'Error',
  'libraries.catalogFallback.title': 'Catálogo no disponible',
  'libraries.catalogFallback.description':
    'Solo se muestran los paquetes registrados localmente. Este no es el catálogo completo.',
  'libraries.state.localOnly': 'Solo local',
  'libraries.state.downloaded': 'Descargado',
  'libraries.state.missing': 'Faltan archivos',
  'libraries.state.corrupt': 'Archivos dañados',
  'libraries.hash.copy': 'Copiar hash',
  'libraries.hash.copied': 'Copiado',
  'libraries.hash.failed': 'Error al copiar',
  'libraries.hash.copiedToast': 'Hash copiado al portapapeles',
  'libraries.sort.asc': 'Orden ascendente',
  'libraries.sort.desc': 'Orden descendente',
  'libraries.sort.none': 'No ordenado',
  'libraries.actions.delete': 'Eliminar',
  'libraries.actions.download': 'Descargar',
  'libraries.actions.deletedToast': 'Eliminado {version}',
  'libraries.actions.downloadedToast': 'Descargado {version}',
  'libraries.actions.failedToast': 'No se pudo {action}',
  'libraries.actions.downloadAll': 'Descargar las últimas',
  'libraries.actions.downloadAllCount': 'Descargar las últimas ({count})',
  'libraries.actions.downloadAllUpToDate': 'Todas las últimas versiones ya están descargadas',
  'libraries.actions.downloadAllTooltip': {
    one: 'Descargar 1 versión más reciente',
    other: 'Descargar {count} versiones más recientes',
  },
  'libraries.actions.downloadAllDoneToast': {
    one: '{count} biblioteca descargada',
    other: '{count} bibliotecas descargadas',
  },
  'libraries.actions.downloadAllPartialToast': '{succeeded} descargadas, {failed} con error',
  'libraries.actions.downloadAllNoneToast': 'Todas las últimas versiones ya están descargadas',

  'common.cancel': 'Cancelar',
  'common.apply': 'Aplicar',

  'filters.title': 'Filtros',
  'filters.launchers.title': 'Lanzadores',
  'filters.launchers.empty': 'No se encontraron lanzadores',
  'filters.launchers.reorder': 'Mover {label}',
  'filters.libraries.title': 'Componentes',
  'filters.libraries.empty': 'No se encontraron componentes',
  'filters.addons.title': 'Complementos',

  'games.favoritesToggle': 'Favoritos',
  'games.favoritesToggleActive': 'Favoritos (activos)',
  'games.showHiddenActive': 'Juegos ocultos (activos)',
  'games.showHidden': 'Mostrar',

  'operation.label.low': 'Riesgo bajo',
  'operation.label.medium': 'Riesgo medio',
  'operation.label.high': 'Riesgo alto',
  'operation.label.blocked': 'Bloqueado',
  'operation.label.planned': 'Planificado',
  'operation.label.completed': 'Completado',
  'operation.label.failed': 'Fallido',
  'operation.label.rolledBack': 'Revertido',
  'operation.label.replaceComponent': 'Cambiar versión',
  'operation.duration': 'Finalizado en {seconds}s',
  'operation.filesUpdated.none': 'Ningún archivo actualizado.',
  'operation.filesUpdated.count': {
    one: '1 archivo actualizado.',
    other: '{count} archivos actualizados.',
  },
  'operation.filesRestored.none': 'Ningún archivo restaurado.',
  'operation.filesRestored.count': {
    one: '1 archivo restaurado.',
    other: '{count} archivos restaurados.',
  },
  'operation.itemAria': '{kind}, {status}',

  'notify.stalePlan': 'El plan de operación está desactualizado. Por favor, inténtalo de nuevo.',
  'notify.missingStableGameId': 'No se pudo identificar el juego.',
  'notify.coverPickerPreview':
    'Por favor, utiliza la aplicación de escritorio para elegir una carátula.',
  'notify.coverUpdated.title': 'Carátula actualizada',
  'notify.coverUpdated.body': 'Tu carátula personalizada se ha guardado.',
  'notify.coverDownloaded.title': 'Carátula descargada',
  'notify.coverDownloaded.body': 'La carátula del juego se ha actualizado.',
  'notify.coverRemoved.title': 'Carátula eliminada',
  'notify.coverRemoved.body': 'Se restauró la carátula predeterminada.',
  'notify.favoriteFailed': 'No se pudo cambiar el estado de favorito.',
  'notify.favoriteAdded': 'Añadido a favoritos.',
  'notify.favoriteRemoved': 'Eliminado de favoritos.',
  'notify.hiddenFailed': 'No se pudo cambiar el estado de oculto.',
  'notify.gameHidden': 'Juego ocultado.',
  'notify.gameUnhidden': 'Juego mostrado.',
  'notify.applyCompleted': 'Cambios aplicados',
  'notify.rollbackCompleted': 'Reversión completada',
  'notify.swapBatchFailed.title': 'Algunas actualizaciones fallaron',
  'notify.swapBatchFailed.description':
    'No se pudieron actualizar {failed} de {total} componentes.',
  'notify.rollbackBatchFailed.title': 'Algunas restauraciones fallaron',
  'notify.rollbackBatchFailed.description':
    'No se pudieron restaurar {failed} de {total} componentes.',
  'notify.statusError': 'Error',
  'notify.statusWarning': 'Advertencia',

  'scan.partialWarning': {
    one: 'No se pudo escanear 1 carpeta.',
    other: 'No se pudieron escanear {count} carpetas.',
  },

  'coverSync.failed': 'No se pudieron sincronizar las carátulas.',
  'coverSync.refreshFailed': 'No se pudieron sincronizar las carátulas.',

  'nvidia.adminRequired': 'Se requieren privilegios de administrador',
  'nvidia.relaunchTo': 'Reinicia como administrador para {action}.',
  'nvidia.action.changeSetting': 'aplicar configuraciones',
  'nvidia.action.revertSetting': 'revertir configuraciones',
  'nvidia.changeSettingFailed': 'No se pudieron aplicar las configuraciones',
  'nvidia.revertDefaultFailed': 'No se pudieron restaurar las configuraciones por defecto',
  'nvidia.revertBaselineFailed': 'No se pudieron restaurar las configuraciones iniciales',

  'indicator.relaunchToToggle': 'Reinicia como administrador para alternar el indicador DLSS.',
  'indicator.changeFailed': 'No se pudo alternar el indicador DLSS',

  'libraries.column.version': 'Versión',
  'libraries.column.hash': 'Hash',
  'libraries.column.signed': 'Firmado',
  'libraries.column.size': 'Tamaño',
  'libraries.column.documents': 'Documentos',
  'libraries.column.actions': 'Acciones',
  'libraries.documents.openForVersion': 'Abrir documentos legales de {name} {version}',
  'libraries.documents.title': 'Documentos legales',
  'libraries.documents.description': 'Se aplican a {name} {version}.',
  'libraries.documents.formatPdf': 'PDF',
  'libraries.documents.formatText': 'Texto',
  'libraries.documents.open': 'Abrir',
  'libraries.documents.openFailed': 'No se pudo abrir el documento',
  'libraries.unsigned': 'No firmado',
  'libraries.invalidDate': 'Fecha no válida',
  'libraries.empty.loading': 'Cargando…',
  'libraries.empty.unavailable': 'No se pudieron cargar las bibliotecas',
  'libraries.empty.none': 'No se encontraron bibliotecas',
  'libraries.error.loadFailed': 'No se pudieron cargar las bibliotecas',
  'libraries.error.refreshFailed': 'No se pudo actualizar el manifiesto',
  'libraries.error.downloadFailed': 'Error al descargar',
  'libraries.error.deleteFailed': 'Error al eliminar',
  'libraries.error.downloadedRefreshFailed':
    'Biblioteca descargada, pero no se pudo actualizar el estado',
  'libraries.error.deletedRefreshFailed':
    'Biblioteca eliminada, pero no se pudo actualizar el estado',

  'settings.catalog.source.steam.aria': 'Descargar carátulas de Steam',
  'settings.catalog.source.steam.title': 'Steam',
  'settings.catalog.source.steam.description': 'Descarga carátulas del catálogo público de Steam.',
  'settings.catalog.source.gog.aria': 'Descargar carátulas de GOG',
  'settings.catalog.source.gog.title': 'GOG',
  'settings.catalog.source.gog.description': 'Descarga carátulas del catálogo oficial de GOG.',
  'settings.catalog.source.steamgriddb.aria': 'Descargar carátulas de SteamGridDB',
  'settings.catalog.source.steamgriddb.title': 'SteamGridDB',
  'settings.catalog.source.steamgriddb.description':
    'Descarga carátulas de la comunidad desde SteamGridDB. Requiere una clave API.',
  'settings.catalog.artworkReadError': 'Error al cargar la configuración de carátulas.',
  'settings.catalog.artworkSaveError': 'Error al guardar la configuración de carátulas.',

  'user_message.invalid_argument': 'Entrada proporcionada no válida.',
  'user_message.invalid_game_reference': 'Juego no encontrado.',
  'user_message.invalid_component_reference': 'Componente no encontrado.',
  'user_message.invalid_artifact_reference': 'Elemento no encontrado.',
  'user_message.invalid_operation_reference': 'Acción no encontrada.',
  'user_message.response_serialization_failed': 'Error al procesar la solicitud.',
  'user_message.plan_changed_rebuild':
    'La tarea está desactualizada. Por favor, inténtalo de nuevo.',
  'user_message.game_not_in_catalog': 'El juego no es compatible.',
  'user_message.operation_not_found': 'Acción no encontrada.',
  'user_message.artifact_not_found': 'Elemento no encontrado.',
  'user_message.component_not_found': 'Componente no encontrado.',
  'user_message.invalid_operation_state': 'Esta acción no está disponible actualmente.',
  'user_message.operation_could_not_complete': 'No se pudo completar la acción.',
  'user_message.command_task_failed': 'No se pudo ejecutar el comando.',
  'user_message.storage_failed': 'La aplicación no pudo leer ni escribir su catálogo.',
  'user_message.provider_failed': 'No se pudo leer una fuente de datos.',
  'user_message.detection_failed': 'La aplicación no pudo analizar los archivos del juego.',
  'user_message.steamgriddb_api_key_missing':
    'Por favor, proporciona una clave API de SteamGridDB en la configuración.',
  'user_message.unsupported_cover_image_type': 'Formato de imagen no compatible.',
  'user_message.cover_download_failed': 'Error al descargar la carátula.',
  'user_message.cover_artwork_not_found': 'No se encontró carátula para este juego.',
  'user_message.cover_file_system_error': 'Error al guardar la carátula en el disco.',
  'user_message.stale_replacement_source':
    'No se pudo aplicar esta actualización porque el archivo de origen se reemplazó o modificó fuera de RenderPilot. Vuelva a seleccionar la versión; es posible que se necesite una descarga.',
  'user_message.nvapi_requires_administrator':
    'Se requieren derechos de administrador para cambiar esta configuración.',

  'suggested_action.refresh_games': 'Actualiza la lista de juegos y vuelve a intentarlo.',
  'suggested_action.reload_game_details': 'Actualiza los detalles del juego y vuelve a intentarlo.',
  'suggested_action.refresh_candidates': 'Actualiza la lista y vuelve a intentarlo.',
  'suggested_action.rebuild_plan_or_reload_operations': 'Actualiza la vista y vuelve a intentarlo.',
  'suggested_action.retry_after_required_data': 'Por favor, espera e inténtalo de nuevo más tarde.',
  'suggested_action.inspect_logs': 'Si el problema persiste, intenta reiniciar la aplicación.',
  'suggested_action.retry_or_restart': 'Si el problema persiste, intenta reiniciar la aplicación.',
  'suggested_action.rebuild_operation_plan': 'Por favor, reinicia la acción.',
  'suggested_action.refresh_or_scan_game_folder':
    'Actualiza la lista o escanea la carpeta de nuevo.',
  'suggested_action.relaunch_as_administrator':
    'Reinicia la aplicación como administrador y vuelve a intentarlo.',

  'settings.about.title': 'Actualizaciones',
  'settings.about.description': 'Buscar actualizaciones de la aplicación.',
  'settings.about.version.title': 'Versión de la aplicación',
  'settings.about.version.loading': 'Cargando...',
  'settings.about.checkForUpdates': 'Buscar actualizaciones',
  'settings.about.updateInProgress': 'Actualizando…',
  'settings.about.updateAvailable': 'Actualización disponible',
  'settings.about.upToDate': 'Tienes la última versión',
  'settings.about.updateCheckError': 'Error al buscar actualizaciones',

  'settings.about.updateDialog.title': 'Actualización disponible',
  'settings.about.updateDialog.versionLine': '{currentVersion} → {version}',
  'settings.about.updateDialog.releaseDate': 'Publicada el {date}',
  'settings.about.updateDialog.releaseNotes': 'Notas de la versión',
  'settings.about.updateDialog.noNotes':
    'No se proporcionaron notas de la versión para esta actualización.',
  'settings.about.updateDialog.notesTruncated': 'Las notas de la versión se acortaron.',

  'settings.about.updateDialog.installAndRestart': 'Instalar y reiniciar',
  'settings.about.updateDialog.later': 'Más tarde',
  'settings.about.updateDialog.close': 'Cerrar',
  'settings.about.updateDialog.retryDownload': 'Reintentar descarga',
  'settings.about.updateDialog.retryInstall': 'Reintentar instalación',
  'settings.about.updateDialog.restartNow': 'Reiniciar ahora',

  'settings.about.updateDialog.downloading': 'Descargando actualización…',
  'settings.about.updateDialog.downloadingBytes': '{received} descargados',
  'settings.about.updateDialog.downloadingBytesTotal': '{received} de {total}',
  'settings.about.updateDialog.verifying': 'Verificando actualización…',
  'settings.about.updateDialog.verifyingDescription': 'Comprobando el paquete descargado.',
  'settings.about.updateDialog.installing':
    'Instalando actualización… La app se cerrará; el instalador puede aparecer un momento.',
  'settings.about.updateDialog.restarting': 'Reiniciando la aplicación…',

  'settings.about.updateDialog.prepareErrorTitle': 'Error de descarga o verificación',
  'settings.about.updateDialog.prepareErrorDescription':
    'No se pudo descargar o verificar la actualización. Comprueba la conexión e inténtalo de nuevo.',
  'settings.about.updateDialog.installErrorTitle': 'Error de instalación',
  'settings.about.updateDialog.installErrorDescription':
    'No se pudo instalar la actualización. Puedes intentarlo de nuevo.',
  'settings.about.updateDialog.restartRequiredTitle': 'Reinicio necesario',
  'settings.about.updateDialog.restartRequiredDescription':
    'La actualización se instaló, pero la aplicación no pudo reiniciarse automáticamente. Reinicia RenderPilot manualmente para finalizar la actualización.',

  'settings.about.updateDialog.progressAria': 'Progreso de descarga: {percent} por ciento',
  'settings.about.updateDialog.indeterminateProgressAria': 'Progreso de descarga',
  // ── Game details: RenoDX ──
  'gameDetails.renodx.title': 'RenoDX HDR',
  'gameDetails.renodx.description':
    'Añade HDR y mapeo de tonos a este juego mediante el complemento ReShade de RenoDX.',
  'gameDetails.renodx.loading': 'Comprobando disponibilidad…',
  'gameDetails.renodx.installError': 'Error al instalar RenoDX',
  'gameDetails.renodx.uninstallError': 'Error al eliminar RenoDX',
  'gameDetails.renodx.switchError': 'Error al cambiar el canal de ReShade',
  'gameDetails.renodx.unsupported': 'No hay un perfil de RenoDX para este juego.',
  'gameDetails.renodx.incompatible': 'No se puede instalar RenoDX: {reason}.',
  'gameDetails.renodx.status.label': 'Status',
  'gameDetails.renodx.statusInstalled': 'Instalado',
  'gameDetails.renodx.actionInstall': 'Instalar',
  'gameDetails.renodx.actionUninstall': 'Quitar RenoDX',
  'gameDetails.renodx.actionRepair': 'Reparar',
  'gameDetails.renodx.uninstallConfirmTitle': '¿Quitar RenoDX de este juego?',
  'gameDetails.renodx.uninstallConfirmBody':
    'Esto elimina el add-on RenoDX y restaura solo los archivos de ReShade modificados durante la configuración de RenoDX.',
  'gameDetails.renodx.uninstallConfirmAction': 'Quitar',
  'gameDetails.renodx.installing': 'Instalando…',
  'gameDetails.renodx.confirmTitle': '¿Instalar RenoDX pese al riesgo de anti-trampas?',
  'gameDetails.renodx.cancel': 'Cancelar',
  // ── Game details: RenoDX shared Vulkan layer ──
  'gameDetails.renodx.vulkanLayer.removeError':
    'No se pudo eliminar la capa Vulkan compartida de ReShade.',
  'gameDetails.renodx.vulkanLayer.title': 'Capa Vulkan compartida',
  'gameDetails.renodx.vulkanLayer.removeConfirmTitle': '¿Eliminar la capa Vulkan compartida?',
  'gameDetails.renodx.vulkanLayer.removeConfirmBody':
    'Eliminar la capa Vulkan de ReShade compartida afecta a todos los juegos Vulkan de RenoDX. ¿Continuar?',
  'gameDetails.renodx.vulkanLayer.openSettings': 'Abrir ajustes de RenoDX',
  'gameDetails.renodx.vulkanLayer.externalReadOnly':
    'Se detectó una capa Vulkan existente; solo lectura en esta versión',
  'gameDetails.renodx.vulkanLayer.state.not_installed': 'No instalada',
  'gameDetails.renodx.vulkanLayer.state.installed': 'Instalada',
  'gameDetails.renodx.vulkanLayer.state.installed_disabled': 'Disabled in registry',
  'gameDetails.renodx.vulkanLayer.state.external_read_only': 'Solo lectura',
  'gameDetails.renodx.vulkanLayer.state.conflict': 'Conflicto',
  'gameDetails.renodx.vulkanLayer.state.needs_repair': 'Necesita reparación',
  'gameDetails.renodx.vulkanLayer.state.unsupported': 'No compatible',
  'gameDetails.renodx.vulkanLayer.action.install': 'Instalar',
  'gameDetails.renodx.vulkanLayer.action.update': 'Actualizar',
  'gameDetails.renodx.vulkanLayer.action.switch_channel': 'Cambiar de canal',
  'gameDetails.renodx.vulkanLayer.action.repair': 'Reparar capa',
  'gameDetails.renodx.vulkanLayer.action.remove': 'Quitar',
  'gameDetails.renodx.vulkanLayer.diagnostic.external_layer_detected':
    'Se detectó una capa Vulkan existente.',
  'gameDetails.renodx.vulkanLayer.diagnostic.duplicate_layer_manifest':
    'Hay varios manifiestos de capa de ReShade registrados.',
  'gameDetails.renodx.vulkanLayer.diagnostic.ambiguous_loader_visibility':
    'La visibilidad del cargador es ambigua.',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_layer_dll': 'Falta la DLL de la capa.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unreadable_dll':
    'The layer DLL could not be read (permission denied or locked).',
  'gameDetails.renodx.vulkanLayer.diagnostic.missing_manifest': 'The layer manifest is missing.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_missing':
    'Los archivos de la capa existen, pero falta el registro del cargador Vulkan.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_disabled':
    'The loader registry entry is disabled.',
  'gameDetails.renodx.vulkanLayer.diagnostic.unsupported_architecture':
    'La arquitectura de la capa no es compatible.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hkcu_not_visible_when_elevated':
    'La capa está registrada en HKCU y puede no cargarse para juegos ejecutados con privilegios elevados.',
  'gameDetails.renodx.vulkanLayer.diagnostic.manifest_malformed':
    'No se pudo analizar un manifiesto de capa.',
  'gameDetails.renodx.vulkanLayer.diagnostic.registry_scope_not_writable':
    'No se puede escribir en el ámbito de registro requerido.',
  'gameDetails.renodx.vulkanLayer.diagnostic.permission_denied':
    'El sistema operativo denegó una operación requerida.',
  'gameDetails.renodx.vulkanLayer.diagnostic.backend_validation_failed':
    'La validación del backend falló; la capa necesita revisión.',
  'gameDetails.renodx.vulkanLayer.diagnostic.hash_mismatch':
    'The layer DLL hash does not match the expected version.',
  'gameDetails.renodx.vulkanLayer.diagnostic.db_only_fallback':
    'The layer DLL is missing; using advisory database record.',
  // ── Game details: RenoDX incompatibility reasons ──
  'gameDetails.renodx.reason.api_unsupported': 'API gráfica no compatible',
  'gameDetails.renodx.reason.api_not_allowed': 'API gráfica no permitida para este juego',
  'gameDetails.renodx.reason.arch_unknown': 'arquitectura del ejecutable desconocida',
  'gameDetails.otherTab': 'Otros',
  'gameDetails.renodx.unavailable': 'RenoDX no está disponible ahora mismo.',
  'renodx.generic.universal': 'RenoDX universal',
  'renodx.generic.unity': 'RenoDX universal (Unity)',
  'gameDetails.renodx.generic.profileTooltip': 'Se está usando un perfil compartido del motor.',
  'renodx.phase.finalizing': 'Finalizando…',
  'luma.phase.finalizing': 'Finalizando…',
  'gameDetails.renodx.confidenceLabel': 'Compatibilidad de RenoDX',
  'gameDetails.renodx.confidenceVerified': 'Funciona',
  'gameDetails.renodx.confidenceExperimental': 'En progreso',
  'gameDetails.renodx.confidenceUntested': 'Sin verificar',
  'gameDetails.renodx.external':
    'Este complemento RenoDX se distribuye externamente y debe descargarse manualmente.',
  'gameDetails.renodx.actionOpenExternal': 'Abrir página de descarga',
  'gameDetails.renodx.external.installFromFile': 'Instalar desde archivo',
  'gameDetails.renodx.external.dropHint':
    'Descarga el complemento y luego suéltalo aquí o elige el archivo.',
  'gameDetails.renodx.external.invalidFile':
    'Ese archivo no es un complemento RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.title': 'Instalación manual',
  'gameDetails.renodx.fileInstall.chooseFile': 'Elegir archivo de add-on…',
  'gameDetails.renodx.fileInstall.chooseAnother': 'Elegir otro archivo',
  'gameDetails.renodx.fileInstall.expected': 'Add-on esperado: {name}',
  'gameDetails.renodx.fileInstall.confirm': '¿Instalar {fileName}?',
  'gameDetails.renodx.fileInstall.errorExtension':
    'Ese archivo no es un add-on de RenoDX (.addon64 / .addon32).',
  'gameDetails.renodx.fileInstall.errorArch':
    'Este add-on es {addon} pero el juego es {game}. Descarga el add-on correspondiente.',
  'gameDetails.renodx.fileInstall.warnName':
    'No parece el add-on esperado ({expected}). Instálalo solo si estás seguro.',
  'gameDetails.renodx.nativeHdr': 'Este juego ya admite HDR nativo: RenoDX no es necesario.',
  'gameDetails.renodx.blacklisted': 'RenoDX no se recomienda para este juego.',
  'gameDetails.renodx.updatesNotTracked': 'Actualizaciones no rastreadas',
  'gameDetails.renodx.channel.label': 'Canal del host ReShade',
  'gameDetails.renodx.channel.hostLabel': 'Host ReShade',
  'gameDetails.renodx.channel.stable': 'Stable',
  'gameDetails.renodx.channel.nightly': 'Nightly',
  'gameDetails.renodx.host.version': '{version}',
  'gameDetails.renodx.host.versionUnknown': 'versión de ReShade desconocida',
  'gameDetails.renodx.host.addons.none': 'complementos no compatibles',
  'gameDetails.renodx.host.addons.unknown': 'compatibilidad de complementos desconocida',
  'gameDetails.renodx.host.action.update_host': 'actualización disponible',
  'gameDetails.renodx.host.action.repair_host':
    'Reparar ReShade para la compatibilidad con add-ons de RenoDX',
  'gameDetails.renodx.host.customBuild':
    'Compilación personalizada (p. ej. GShade) — la actualizas tú mismo',
  'gameDetails.renodx.host.conflictMultiple':
    'Se encontraron varios hosts de ReShade: revisa la ranura activa',
  'gameDetails.renodx.host.conflictBlocksInstall':
    'Un archivo existente ocupa la ranura de ReShade que usa este juego, o ReShade está en otra ranura: resuélvelo antes de instalar.',
  'gameDetails.renodx.actionUpdate': 'Actualizar',
  'gameDetails.renodx.updating': 'Actualizando…',
  'gameDetails.renodx.updateError': 'Error al actualizar RenoDX',
  'gameDetails.renodx.actionInstallDlssFix': 'Instalar',
  'gameDetails.renodx.actionRemoveDlssFix': 'Quitar',
  'gameDetails.renodx.dlssFixInstallError': 'Error al instalar DLSS-Fix',
  'gameDetails.renodx.dlssFixRemoveError': 'Error al desinstalar DLSS-Fix',
  'gameDetails.renodx.fresh.label': 'Actualizaciones',
  'gameDetails.renodx.fresh.current': 'Actualizado',
  'gameDetails.renodx.fresh.available': 'Actualización disponible',
  'gameDetails.renodx.fresh.channelMismatch': 'Cambio de canal disponible',
  'gameDetails.renodx.fresh.validationRequired': 'Se requiere validación',
  'gameDetails.renodx.fresh.unknown': 'No se pudo comprobar',
  'gameDetails.renodx.fresh.checking': 'Comprobando…',
  'gameDetails.renodx.addonDated': 'Add-on del {date}',
  'gameDetails.renodx.installedOn': 'Instalado el {date}',
  'gameDetails.renodx.lastChecked': 'Comprobado {time}',
  'gameDetails.renodx.lastCheckedNever': 'Sin comprobar aún',
  'gameDetails.renodx.actionCheckUpdates': 'Buscar actualizaciones',
  'gameDetails.renodx.component.reshade': 'Host de ReShade',
  'gameDetails.renodx.component.addon': 'Add-on de RenoDX',
  'gameDetails.renodx.component.addonDesc': 'El add-on HDR para este juego',
  'gameDetails.renodx.component.addonDisabled': 'Instalado, pero desactivado en ReShade.ini',
  'gameDetails.renodx.component.addonFileInstall':
    'Instalado desde un archivo — sin seguimiento de actualizaciones',
  'gameDetails.renodx.component.dlssFix': 'DLSS-Fix',
  'gameDetails.renodx.component.dlssFixDesc': 'Corrige el parpadeo con DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixOffer':
    'Disponible — evita el parpadeo con DLSS Frame Generation',
  'gameDetails.renodx.component.dlssFixHint':
    'Una corrección general de ReShade, no específica de RenoDX. Hace que ReShade dibuje sobre los fotogramas nativos del juego en lugar de los de Frame Generation, y oculta el escalado DLSS a ReShade cuando el juego implementa Streamline correctamente.',
  'gameDetails.renodx.attribution': 'RenoDX por clshortfuse.',
  'gameDetails.renodx.attributionLink': 'Ver proyecto',
  // ── Game details: shared add-on copy (RenoDX + Luma) ──
  'gameDetails.addon.riskSafe': 'No se detectó anti-cheat — la instalación es segura.',
  'gameDetails.addon.riskWarn':
    'Se detectó anti-cheat — la instalación podría provocar una expulsión.',
  'addon.risk.sp_safe':
    'No se encontraron firmas anti-trampas conocidas — instalar {addonName} probablemente sea seguro, pero no está garantizado.',
  'addon.risk.anticheat_detected':
    'Se detectaron firmas anti-trampas — instalar {addonName} podría provocar una expulsión.',
  'gameDetails.addon.confirmAccept': 'Instalar de todos modos',
  'gameDetails.addon.confirmBody':
    'Este juego usa anti-cheat. El complemento ReShade podría activarlo y provocar una expulsión. Continúa bajo tu propio riesgo.',
  'gameDetails.addon.fullAddonWarning':
    'El soporte completo de complementos de ReShade puede ser inseguro en juegos multijugador o protegidos por anti-cheat.',
  'gameDetails.addon.blockedByOtherAddon.tracked':
    '{installedAddon} está instalado para este juego — desinstálalo antes de instalar {blockedAddon}.',
  'gameDetails.addon.blockedByOtherAddon.unmanaged':
    'Se encontraron archivos de {installedAddon} en el disco para este juego — elimínalos antes de instalar {blockedAddon}.',
  'addon.availability.loadFailed': 'No se pudo comprobar',
  'addon.availability.retry': 'Reintentar',
  'addon.availability.checking': 'Comprobando…',
  // ── Game details: Luma ──
  'gameDetails.luma.title': 'Luma Framework',
  'gameDetails.luma.description':
    'Las funciones de Luma disponibles para este juego se muestran a continuación.',
  'gameDetails.luma.loading': 'Comprobando disponibilidad…',
  'gameDetails.luma.installError': 'Error al instalar Luma',
  'gameDetails.luma.uninstallError': 'Error al desinstalar Luma',
  'gameDetails.luma.updateError': 'Error al actualizar Luma',
  'gameDetails.luma.repairError': 'Error al reparar Luma',
  'gameDetails.luma.unsupported': 'No hay ningún perfil de Luma disponible para este juego.',
  'gameDetails.luma.incompatible': 'Luma no se puede instalar: {reason}.',
  'gameDetails.luma.blacklisted': 'Luma no se recomienda para este juego.',
  'gameDetails.luma.unavailable': 'Luma no está disponible en este momento.',
  'gameDetails.luma.unmanagedPresent':
    'Se encontró una instalación de Luma existente en el disco sin registro asociado. Elimínala manualmente y luego reinstala.',
  'gameDetails.luma.installTornWarning':
    'Una instalación anterior no terminó correctamente. Instalar de nuevo la limpiará y reparará.',
  'gameDetails.luma.installTornWarningInstalled':
    'La última operación no terminó correctamente. Usa Reparar (o Actualizar si aparece) para terminar de reconciliar la instalación.',
  'gameDetails.luma.status.label': 'Estado',
  'gameDetails.luma.statusInstalled': 'Instalado',
  'gameDetails.luma.actionInstall': 'Instalar',
  'gameDetails.luma.installing': 'Instalando…',
  'gameDetails.luma.actionUninstall': 'Quitar Luma',
  'gameDetails.luma.actionRepair': 'Reparar',
  'gameDetails.luma.actionUpdate': 'Actualizar',
  'gameDetails.luma.updating': 'Actualizando…',
  'gameDetails.luma.actionCheckUpdates': 'Buscar actualizaciones',
  'gameDetails.luma.uninstallConfirmTitle': '¿Quitar Luma de este juego?',
  'gameDetails.luma.uninstallConfirmBody':
    'Esto quita Luma. Si Luma administra la DLL de DLSS, se revierte su Library Swap y se restaura exactamente el estado anterior a Luma. Las DLL reutilizadas y los swaps independientes no cambian.',
  'gameDetails.luma.uninstallConfirmAction': 'Quitar',
  'gameDetails.luma.confirmTitle': '¿Instalar Luma a pesar del riesgo de anti-cheat?',
  'gameDetails.luma.vcredistWarning':
    'Es posible que falte un Visual C++ Redistributable reciente en este sistema. Si Luma no carga, instala el redistribuible.',
  'gameDetails.luma.vcredistLink': 'Descargar el redistribuible',
  'gameDetails.luma.dgvoodoo.managed':
    'RenderPilot instalará y configurará dgVoodoo2 {version} para este perfil de Luma.',
  // ── Game details: Luma confidence ──
  'gameDetails.luma.confidenceLabel': 'Compatibilidad con Luma',
  'gameDetails.luma.confidenceVerified': 'Funciona',
  'gameDetails.luma.confidenceExperimental': 'En progreso',
  'gameDetails.luma.confidenceUntested': 'Sin verificar',
  'gameDetails.luma.generic.engineUnreal': 'Unreal Engine',
  'gameDetails.luma.generic.engineUnity': 'Unity',
  'gameDetails.luma.generic.profileTooltip': 'Se está usando un perfil compartido del motor.',
  'gameDetails.luma.features.title': 'Funciones',
  'gameDetails.luma.features.dlssFsr': 'DLSS / FSR',
  'gameDetails.luma.features.hdr': 'HDR',
  'gameDetails.luma.features.supported': 'Compatible',
  'gameDetails.luma.features.unsupported': 'No compatible',
  'gameDetails.luma.features.experimental': 'Experimental',
  'gameDetails.luma.features.unknown': 'Desconocido',
  // ── Game details: Luma incompatibility reasons ──
  'gameDetails.luma.reason.api_unsupported': 'API gráfica no compatible',
  'gameDetails.luma.reason.api_not_allowed': 'API gráfica no permitida para este juego',
  'gameDetails.luma.reason.arch_unknown': 'arquitectura del ejecutable desconocida',
  'gameDetails.luma.reason.arch_mismatch':
    'la arquitectura del ejecutable no coincide con este complemento',
  // ── Game details: Luma ReShade host ──
  'gameDetails.luma.channel.stable': 'Stable',
  'gameDetails.luma.channel.nightly': 'Nightly',
  'gameDetails.luma.host.version': '{version}',
  'gameDetails.luma.host.versionUnknown': 'Versión desconocida',
  'gameDetails.luma.host.addons.none': 'complementos no compatibles',
  'gameDetails.luma.host.addons.unknown': 'soporte de complementos desconocido',
  'gameDetails.luma.host.action.update_host': 'actualización disponible',
  'gameDetails.luma.host.action.repair_host':
    'Reparar ReShade para el soporte de complementos de Luma',
  'gameDetails.luma.host.customBuild':
    'Compilación personalizada (p. ej. GShade) — tú gestionas sus actualizaciones',
  'gameDetails.luma.host.conflictMultiple':
    'Se encontraron varios hosts de ReShade — hay que revisar el slot activo',
  'gameDetails.luma.host.conflictBlocksInstall':
    'Un archivo existente ocupa el slot de ReShade que usa este juego, o ReShade está en otro slot — resuélvelo antes de instalar.',
  // ── Game details: Luma freshness / timestamps ──
  'gameDetails.luma.fresh.label': 'Versión',
  'gameDetails.luma.fresh.current': 'Última',
  'gameDetails.luma.fresh.available': 'Actualización disponible',
  'gameDetails.luma.fresh.channelMismatch': 'Cambio de canal disponible',
  'gameDetails.luma.fresh.validationRequired': 'Se requiere validación',
  'gameDetails.luma.fresh.unknown': 'No se pudo comprobar',
  'gameDetails.luma.fresh.checking': 'Comprobando…',
  'gameDetails.luma.updatesNotTracked': 'Actualizaciones no rastreadas',
  'gameDetails.luma.addonDated': 'Complemento del {date}',
  'gameDetails.luma.installedOn': 'Instalado el {date}',
  'gameDetails.luma.lastChecked': 'Comprobado {time}',
  'gameDetails.luma.lastCheckedNever': 'Aún no comprobado',
  // ── Game details: Luma components ──
  'gameDetails.luma.component.reshade': 'Host de ReShade',
  'gameDetails.luma.component.addon': 'Complemento Luma',
  'gameDetails.luma.component.addonDesc': 'Funciones de Luma para este juego',
  'gameDetails.luma.component.dgvoodoo': 'Wrapper dgVoodoo2',
  'gameDetails.luma.component.dgvoodooDesc': 'Puente D3D9 gestionado, versión {version}',
  // ── Game details: Luma launch arguments ──
  'gameDetails.luma.launchArgs.instructions.steam':
    'Si inicias el juego mediante Steam, añádelos allí: clic derecho en el juego → Propiedades → General → Opciones de inicio.',
  'gameDetails.luma.launchArgs.instructions.gog':
    'Si inicias el juego mediante GOG Galaxy, añádelos allí: ajustes del juego → Gestionar instalación → Configurar.',
  'gameDetails.luma.launchArgs.instructions.epic':
    'Si inicias el juego mediante Epic Games Launcher, añádelos allí: clic derecho en el juego → Gestionar → Argumentos de línea de comandos adicionales.',
  'gameDetails.luma.launchArgs.instructions.ea':
    'Si inicias el juego mediante EA app, añádelos allí: selecciona el juego → Gestionar → Ver propiedades → Opciones de inicio avanzadas.',
  'gameDetails.luma.launchArgs.instructions.ubisoft':
    'Si inicias el juego mediante Ubisoft Connect, añádelos allí: selecciona el juego → Propiedades → Añadir argumentos de inicio.',
  'gameDetails.luma.launchArgs.instructions.other':
    'Usa el método que realmente inicia el juego. Añade los argumentos en su launcher, destino del acceso directo, archivo por lotes u otro cargador.',
  'gameDetails.luma.launchArgs.title': 'Se requieren argumentos de inicio',
  'gameDetails.luma.launchArgs.dx11Title': 'Este perfil de Luma requiere DirectX 11',
  'gameDetails.luma.launchArgs.copyStep': 'Copia los argumentos de inicio requeridos:',
  'gameDetails.luma.launchArgs.copy': 'Copiar argumentos',
  'gameDetails.luma.launchArgs.copied': 'Copiado',
  'gameDetails.luma.launchArgs.copyFailed': 'No se pudieron copiar los argumentos de inicio',
  // ── Game details: Luma attribution ──
  'gameDetails.luma.attribution': 'Luma Framework por Filoppi.',
  'gameDetails.luma.attributionLink': 'Ver proyecto',
  'gameDetails.luma.guidance.gameSetting': 'Ajuste del juego',
  'gameDetails.luma.guidance.engineIni': 'Cambio manual de INI',
  'gameDetails.luma.guidance.launchArgument': 'Argumento de inicio',
  'gameDetails.luma.guidance.warning': 'Importante',
  'gameDetails.luma.guidance.compatibility': 'Nota de compatibilidad',
  'gameDetails.luma.guidance.externalTool': 'Herramienta de terceros',
  'gameDetails.luma.guidance.copy': 'Copiar',
  'gameDetails.luma.guidance.copied': 'Copiado',
  'gameDetails.luma.guidance.copyFailed': 'No se pudo copiar',
};
