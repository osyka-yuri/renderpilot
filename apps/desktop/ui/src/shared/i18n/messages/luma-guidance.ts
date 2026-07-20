import type { Locale } from '../locale';

/**
 * Localized, reviewed Luma guidance. The manifest remains the single English
 * source of truth: English deliberately falls back to `fallback_text`.
 *
 * Multiple game-specific IDs often share one wording. Grouping them keeps
 * translation maintenance concise while expanding every current stable ID.
 */
const LUMA_GUIDANCE_GROUPS = {
  sherlockDx11Performance: ['luma.sherlock-holmes-chapter-one.warning'],
  guiltyGearStriveAa: ['luma.guilty-gear-strive.warning'],
  manualLaunchArgument: [
    'luma.abiotic-factor.launch_argument',
    'luma.dead-island-2.launch_argument',
    'luma.gylt.launch_argument',
    'luma.hellblade-senuas-sacrifice.launch_argument',
    'luma.life-is-strange-true-colors.launch_argument',
    'luma.psychonauts-2.launch_argument',
    'luma.redout.launch_argument',
    'luma.redout-2.launch_argument',
    'luma.the-ascent.launch_argument',
    'luma.the-dark-pictures-man-of-medan.launch_argument',
    'luma.witch-it.launch_argument',
    'luma.sherlock-holmes-chapter-one.launch_argument',
    'luma.song-of-nunu.launch_argument',
  ],
  manualEngineIni: [
    'luma.ace-combat-7.engine_ini',
    'luma.ancestors-legacy.engine_ini',
    'luma.circus-electrique.engine_ini',
    'luma.cygni-all-guns-blazing.engine_ini',
    'luma.dnf-duel.engine_ini',
    'luma.dragon-quest-11-s.engine_ini',
    'luma.just-die-already.engine_ini',
    'luma.kingdom-hearts-3.engine_ini',
    'luma.mothergunship.engine_ini',
    'luma.project-wingman.engine_ini',
    'luma.remnant-from-the-ashes.engine_ini',
    'luma.styx-shards-of-darkness.engine_ini',
    'luma.tekken-7.engine_ini',
    'luma.tetris-effect-connected.engine_ini',
    'luma.tribes-of-midgard.engine_ini',
    'luma.warhammer-40000-boltgun.engine_ini',
    'luma.what-remains-of-edith-finch.engine_ini',
  ],
  publicMatchmaking: ['luma.cod-black-ops-3.warning'],
  edithFinchExit: ['luma.what-remains-of-edith-finch.compatibility'],
  dlssNoHdr: [
    'luma.greedfall.warning',
    'luma.batman-arkham-knight.warning',
    'luma.deus-ex-mankind-divided.warning',
  ],
  kh3Txaa: ['luma.kingdom-hearts-3.game_setting'],
  aceFxaaHigh: ['luma.ace-combat-7.game_setting'],
  tetrisFxaa6: ['luma.tetris-effect-connected.game_setting'],
  projectWingmanFxaa: ['luma.project-wingman.game_setting'],
  dnfCharacterSelection: ['luma.dnf-duel.game_setting'],
  tekkenNoD3D9Ex: ['luma.tekken-7.game_setting'],
  scornOptiscaler: ['luma.scorn.warning'],
  hatsuneExclusiveFullscreen: ['luma.hatsune-miku-project-diva-mega-mix-plus.warning'],
  deadlineUltra: ['luma.deadline-delivery.game_setting'],
  filamentAaHigh: ['luma.filament.game_setting'],
  aaHigh: ['luma.kao-the-kangaroo.game_setting', 'luma.submerged-hidden-depths.game_setting'],
  mutantMotionBlur: ['luma.mutant-year-zero-road-to-eden.game_setting'],
  supralandTaa: ['luma.supraland.game_setting'],
  scarletNexusTxaa: ['luma.scarlet-nexus.game_setting'],
  closeToSunAa4x: ['luma.close-to-the-sun.game_setting'],
  darksidersAaEpic: ['luma.darksiders-3.game_setting'],
  codeVeinAaHighest: ['luma.code-vein.game_setting'],
  orcsAaHigh: ['luma.orcs-must-die-3.game_setting'],
  clashAaVeryHigh: ['luma.clash-artifacts-of-chaos.game_setting'],
  vampyrTxaa6x: ['luma.vampyr.game_setting'],
  crashAaMedium: ['luma.crash-bandicoot-4.game_setting'],
  callSeaEpic: ['luma.call-of-the-sea.game_setting'],
  spiritNorthUltra: ['luma.spirit-of-the-north.game_setting'],
  goatHighAa: ['luma.goat-simulator-3.game_setting'],
  crabHighAntialiasing: ['luma.crab-champions.game_setting'],
  spyroHighTaa: ['luma.spyro-reignited-trilogy.game_setting'],
  dieYoungTaa: ['luma.die-young.game_setting'],
  kakarotBdzKfix: ['luma.dragon-ball-z-kakarot.game_setting'],
  preyData: ['luma.prey-2017.prey_extra_data'],
  bundledDlss: [
    'luma.cod-black-ops-3.bundled_dlss',
    'luma.dishonored-2.bundled_dlss',
    'luma.final-fantasy-7-remake.bundled_dlss',
    'luma.granblue-fantasy-relink.bundled_dlss',
    'luma.greedfall.bundled_dlss',
    'luma.just-cause-3.bundled_dlss',
    'luma.mafia-3.bundled_dlss',
    'luma.persona-5-royal.bundled_dlss',
    'luma.prey-2017.bundled_dlss',
    'luma.watch-dogs-2.bundled_dlss',
    'luma.quantum-break.bundled_dlss',
    'luma.mass-effect-andromeda.bundled_dlss',
    'luma.abiotic-factor.bundled_dlss',
    'luma.abzu.bundled_dlss',
    'luma.ace-combat-7.bundled_dlss',
    'luma.agony-unrated.bundled_dlss',
    'luma.alchemy-garden.bundled_dlss',
    'luma.aliens-fireteam-elite.bundled_dlss',
    'luma.ancestors-the-humankind-odyssey.bundled_dlss',
    'luma.ancestors-legacy.bundled_dlss',
    'luma.ashen.bundled_dlss',
    'luma.bloodstained-ritual-of-the-night.bundled_dlss',
    'luma.call-of-the-sea.bundled_dlss',
    'luma.chess-ultra.bundled_dlss',
    'luma.circus-electrique.bundled_dlss',
    'luma.code-vein.bundled_dlss',
    'luma.coral-island.bundled_dlss',
    'luma.crab-champions.bundled_dlss',
    'luma.crash-bandicoot-4.bundled_dlss',
    'luma.clash-artifacts-of-chaos.bundled_dlss',
    'luma.close-to-the-sun.bundled_dlss',
    'luma.cygni-all-guns-blazing.bundled_dlss',
    'luma.darksiders-3.bundled_dlss',
    'luma.daymare-1998.bundled_dlss',
    'luma.dead-island-2.bundled_dlss',
    'luma.deadline-delivery.bundled_dlss',
    'luma.desolate.bundled_dlss',
    'luma.devils-hunt.bundled_dlss',
    'luma.die-young.bundled_dlss',
    'luma.dnf-duel.bundled_dlss',
    'luma.dragon-ball-z-kakarot.bundled_dlss',
    'luma.dragon-quest-11-s.bundled_dlss',
    'luma.eriksholm-the-stolen-dream.bundled_dlss',
    'luma.filament.bundled_dlss',
    'luma.ghostrunner.bundled_dlss',
    'luma.goat-simulator-3.bundled_dlss',
    'luma.grounded.bundled_dlss',
    'luma.guilty-gear-strive.bundled_dlss',
    'luma.gylt.bundled_dlss',
    'luma.hellblade-senuas-sacrifice.bundled_dlss',
    'luma.it-takes-two.bundled_dlss',
    'luma.just-die-already.bundled_dlss',
    'luma.kao-the-kangaroo.bundled_dlss',
    'luma.kingdom-hearts-3.bundled_dlss',
    'luma.layers-of-fear-2.bundled_dlss',
    'luma.life-is-strange-2.bundled_dlss',
    'luma.life-is-strange-true-colors.bundled_dlss',
    'luma.little-nightmares.bundled_dlss',
    'luma.maneater.bundled_dlss',
    'luma.mothergunship.bundled_dlss',
    'luma.mordhau.bundled_dlss',
    'luma.mutant-year-zero-road-to-eden.bundled_dlss',
    'luma.orcs-must-die-3.bundled_dlss',
    'luma.project-wingman.bundled_dlss',
    'luma.psychonauts-2.bundled_dlss',
    'luma.redeemer-enhanced-edition.bundled_dlss',
    'luma.redout.bundled_dlss',
    'luma.redout-2.bundled_dlss',
    'luma.relicta.bundled_dlss',
    'luma.remnant-from-the-ashes.bundled_dlss',
    'luma.ruiner.bundled_dlss',
    'luma.scarlet-nexus.bundled_dlss',
    'luma.scorn.bundled_dlss',
    'luma.selfloss.bundled_dlss',
    'luma.shin-megami-tensei-5-vengeance.bundled_dlss',
    'luma.shenmue-3.bundled_dlss',
    'luma.snake-pass.bundled_dlss',
    'luma.spirit-of-the-north.bundled_dlss',
    'luma.spyro-reignited-trilogy.bundled_dlss',
    'luma.star-wars-jedi-fallen-order.bundled_dlss',
    'luma.state-of-decay-2.bundled_dlss',
    'luma.steel-rats.bundled_dlss',
    'luma.stray.bundled_dlss',
    'luma.styx-shards-of-darkness.bundled_dlss',
    'luma.submerged-hidden-depths.bundled_dlss',
    'luma.succubus.bundled_dlss',
    'luma.supraland.bundled_dlss',
    'luma.tales-of-arise.bundled_dlss',
    'luma.tandem-a-tale-of-shadows.bundled_dlss',
    'luma.tekken-7.bundled_dlss',
    'luma.tell-me-why.bundled_dlss',
    'luma.tetris-effect-connected.bundled_dlss',
    'luma.the-ascent.bundled_dlss',
    'luma.the-awesome-adventures-of-captain-spirit.bundled_dlss',
    'luma.the-dark-pictures-man-of-medan.bundled_dlss',
    'luma.the-gunk.bundled_dlss',
    'luma.the-pathless.bundled_dlss',
    'luma.the-sinking-city.bundled_dlss',
    'luma.tiny-tinas-wonderlands.bundled_dlss',
    'luma.trek-to-yomi.bundled_dlss',
    'luma.tribes-of-midgard.bundled_dlss',
    'luma.tyrants-realm.bundled_dlss',
    'luma.vampyr.bundled_dlss',
    'luma.warhammer-40000-boltgun.bundled_dlss',
    'luma.weird-west.bundled_dlss',
    'luma.what-remains-of-edith-finch.bundled_dlss',
    'luma.witch-it.bundled_dlss',
    'luma.sherlock-holmes-chapter-one.bundled_dlss',
    'luma.song-of-nunu.bundled_dlss',
    'luma.batman-arkham-knight.bundled_dlss',
    'luma.blue-reflection-second-light.bundled_dlss',
    'luma.deus-ex-mankind-divided.bundled_dlss',
    'luma.fallout-4.bundled_dlss',
    'luma.kingdom-come-deliverance.bundled_dlss',
    'luma.metaphor-refantazio.bundled_dlss',
    'luma.monster-hunter-world.bundled_dlss',
    'luma.mortal-kombat-11.bundled_dlss',
    'luma.the-evil-within-2.bundled_dlss',
  ],
  daymareOptiscalerUuu: ['luma.daymare-1998.compatibility'],
  smtLyallFix: ['luma.shin-megami-tensei-5-vengeance.warning'],
  deusExBorisEnb: ['luma.deus-ex-human-revolution.compatibility'],
  xboxStore: ['luma.inside.compatibility', 'luma.lara-croft-temple-of-osiris.compatibility'],
  massEffectNativeAa: ['luma.mass-effect-andromeda.compatibility'],
  metaphorNativeAa: ['luma.metaphor-refantazio.compatibility'],
  itTakesTwoTitle: ['luma.it-takes-two.warning'],
  talesAriseSdk: ['luma.tales-of-arise.warning'],
  metroWindowed: ['luma.metro-2033-redux.warning'],
  edithFinch4k: ['luma.what-remains-of-edith-finch.warning'],
  sinkingCityOriginal: ['luma.the-sinking-city.warning'],
  heavyRainSteamUltrawide: ['luma.heavy-rain.warning'],
  metroBorderless: ['luma.metro-2033-redux.windowed_borderless_only'],
} as const;

type LumaGuidancePhrase = keyof typeof LUMA_GUIDANCE_GROUPS;
type LumaGuidanceLocale = Exclude<Locale, 'en'>;
type LumaGuidanceTranslations = Record<LumaGuidancePhrase, string>;

const translations = {
  ru: {
    sherlockDx11Performance:
      'Аргумент запуска -dx11 может снизить производительность CPU в режиме DX11. При DLAA с AutoExposure:On на траве могут появляться зубчатые края.',
    guiltyGearStriveAa:
      'Сглаживание не работает на экране выбора персонажа. В игре выберите AA «Temporal Anti Aliasing», затем внесите в Engine.ini: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
    manualLaunchArgument: 'Добавьте этот аргумент запуска вручную.',
    manualEngineIni: 'Вручную добавьте в Engine.ini следующие настройки.',
    publicMatchmaking:
      'Не используйте официальный публичный матчмейкинг, пока Luma установлена. Это может привести к бану.',
    edithFinchExit:
      'DLAA работает без дополнительных изменений, но игра может не завершаться полностью после выхода. OptiScaler может устранить эту проблему.',
    dlssNoHdr: 'Только DLSS (HDR пока не поддерживается).',
    kh3Txaa: 'В игре предварительно выберите «TXAA».',
    aceFxaaHigh: 'В игре предварительно выберите AA «FXAA High».',
    tetrisFxaa6: 'В игре предварительно выберите AA «FXAA:6» и масштаб рендеринга 100%.',
    projectWingmanFxaa: 'В игре предварительно выберите AA «FXAA».',
    dnfCharacterSelection: 'Сглаживание не работает на экране выбора персонажа.',
    tekkenNoD3D9Ex: 'В качестве предварительной настройки добавьте аргумент запуска -nod3d9ex.',
    scornOptiscaler:
      'В игре есть нативная поддержка FSR 2.1, поэтому DLSS или другой апскейлер можно добавить через OptiScaler.',
    hatsuneExclusiveFullscreen:
      'При проблемах не используйте эксклюзивный полноэкранный режим. Чтобы выйти из него, нажмите Alt+Enter.',
    deadlineUltra: 'В настройках игры выберите «Ultra».',
    filamentAaHigh: 'В настройках игры выберите AA «High» или «Very High».',
    aaHigh: 'В настройках игры выберите AA «High».',
    mutantMotionBlur:
      'В настройках игры выберите AA «High». Для более чёткого движения рекомендуется задать r.motionblur.amount=0 в Engine.ini.',
    supralandTaa: 'В настройках игры выберите AA «Temporal Anti Aliasing».',
    scarletNexusTxaa: 'В настройках игры выберите AA «TXAA».',
    closeToSunAa4x: 'В настройках игры выберите AA 4X.',
    darksidersAaEpic: 'В настройках игры выберите AA Epic.',
    codeVeinAaHighest: 'В настройках игры выберите AA Highest.',
    orcsAaHigh: 'В настройках игры выберите качество AA «High».',
    clashAaVeryHigh: 'В настройках игры выберите качество AA «Very High».',
    vampyrTxaa6x: 'В настройках игры выберите AA TXAA 6X.',
    crashAaMedium: 'В настройках игры выберите качество сглаживания не ниже Medium (2x).',
    callSeaEpic: 'В настройках игры выберите общее качество «Epic».',
    spiritNorthUltra: 'В настройках игры выберите качество графики «Ultra».',
    goatHighAa: 'В настройках игры выберите High AA.',
    crabHighAntialiasing: 'В настройках игры выберите High Anti-aliasing Type.',
    spyroHighTaa: 'В настройках игры выберите High TAA.',
    dieYoungTaa: 'В настройках игры выберите TAA «High» или «Epic».',
    kakarotBdzKfix:
      'В настройках игры выберите TAA и используйте BDZKFix для Legacy-версии либо его обновлённый форк для HD-версии.',
    preyData: 'Дополнительные файлы данных Luma для Prey должны оставаться рядом с аддоном.',
    bundledDlss:
      'Luma резервирует встроенную библиотеку DLSS игры и восстанавливает её при удалении.',
    daymareOptiscalerUuu: 'Luma работает сама по себе, но вылетает вместе с OptiScaler или UUU.',
    smtLyallFix: "Для принудительного TAA требуется Lyall's Fix.",
    deusExBorisEnb:
      'Несовместимо с Boris ENB (DX9). Работает с DE и оригинальным изданием. Мод Gold Filter Restoration здесь не нужен.',
    xboxStore: 'Несовместимо с версией из Xbox Store.',
    massEffectNativeAa:
      'Доступны только режимы DLAA / FSR 3 Native AA; это не суперразрешение DLSS или FSR.',
    metaphorNativeAa:
      'Доступны только режимы DLAA / FSR Native AA; это не суперразрешение DLSS или FSR.',
    itTakesTwoTitle: 'Работает только в последовательности на титульном экране.',
    talesAriseSdk: 'Требуется Arise-SDK с параметром UseUE4TAA = true.',
    metroWindowed:
      'Требуется оконный или безрамочный режим: через моды либо отключением полноэкранного режима в конфиге игры.',
    edithFinch4k:
      'Игра работает некорректно в 4K. Перед ручным изменением Engine.ini установите Effects на Low.',
    sinkingCityOriginal: 'Оригинальная версия работает. Совместимость ремастера неизвестна.',
    heavyRainSteamUltrawide: 'Ультраширокий режим может работать только при запуске через Steam.',
    metroBorderless: 'Используйте безрамочный оконный режим.',
  },
  de: {
    sherlockDx11Performance:
      'Der Startparameter -dx11 kann die CPU-Leistung im DX11-Modus verringern. Bei DLAA mit AutoExposure:On können an Gras gezackte Kanten auftreten.',
    guiltyGearStriveAa:
      'Antialiasing funktioniert im Charakterauswahlbildschirm nicht. Wähle im Spiel AA „Temporal Anti Aliasing“ und ergänze Engine.ini um: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
    manualLaunchArgument: 'Füge dieses Startargument manuell hinzu.',
    manualEngineIni: 'Übernimm die folgenden Einstellungen manuell in Engine.ini.',
    publicMatchmaking:
      'Vermeide offizielles öffentliches Matchmaking, solange Luma installiert ist. Dies kann zu einer Sperre führen.',
    edithFinchExit:
      'DLAA funktioniert ohne weitere Änderungen, doch das Spiel schließt sich nach dem Beenden möglicherweise nicht vollständig. OptiScaler kann das Problem beheben.',
    dlssNoHdr: 'Nur DLSS (derzeit kein HDR).',
    kh3Txaa: 'Wähle im Spiel zuvor „TXAA“ aus.',
    aceFxaaHigh: 'Wähle im Spiel zuvor AA „FXAA High“ aus.',
    tetrisFxaa6: 'Wähle im Spiel zuvor AA „FXAA:6“ und 100 % Render-Skalierung aus.',
    projectWingmanFxaa: 'Wähle im Spiel zuvor AA „FXAA“ aus.',
    dnfCharacterSelection: 'Antialiasing funktioniert im Charakterauswahlbildschirm nicht.',
    tekkenNoD3D9Ex: 'Füge als Voraussetzung das Startargument -nod3d9ex hinzu.',
    scornOptiscaler:
      'Das Spiel unterstützt FSR 2.1 nativ; DLSS oder einen anderen Upscaler kannst du daher über OptiScaler hinzufügen.',
    hatsuneExclusiveFullscreen:
      'Nutze bei Problemen keinen exklusiven Vollbildmodus. Drücke Alt+Enter, um ihn zu verlassen.',
    deadlineUltra: 'Wähle in den Spieleinstellungen „Ultra“ aus.',
    filamentAaHigh: 'Wähle in den Spieleinstellungen AA „High“ oder „Very High“ aus.',
    aaHigh: 'Wähle in den Spieleinstellungen AA „High“ aus.',
    mutantMotionBlur:
      'Wähle in den Spieleinstellungen AA „High“ aus. Für klarere Bewegung wird r.motionblur.amount=0 in Engine.ini empfohlen.',
    supralandTaa: 'Wähle in den Spieleinstellungen AA „Temporal Anti Aliasing“ aus.',
    scarletNexusTxaa: 'Wähle in den Spieleinstellungen AA „TXAA“ aus.',
    closeToSunAa4x: 'Wähle in den Spieleinstellungen AA 4X aus.',
    darksidersAaEpic: 'Wähle in den Spieleinstellungen AA Epic aus.',
    codeVeinAaHighest: 'Wähle in den Spieleinstellungen AA Highest aus.',
    orcsAaHigh: 'Wähle in den Spieleinstellungen die AA-Qualität „High“ aus.',
    clashAaVeryHigh: 'Wähle in den Spieleinstellungen die AA-Qualität „Very High“ aus.',
    vampyrTxaa6x: 'Wähle in den Spieleinstellungen AA TXAA 6X aus.',
    crashAaMedium:
      'Wähle in den Spieleinstellungen mindestens die Antialiasing-Qualität Medium (2x) aus.',
    callSeaEpic: 'Wähle in den Spieleinstellungen die Gesamtqualität „Epic“ aus.',
    spiritNorthUltra: 'Wähle in den Spieleinstellungen die Grafikqualität „Ultra“ aus.',
    goatHighAa: 'Wähle in den Spieleinstellungen High AA aus.',
    crabHighAntialiasing: 'Wähle in den Spieleinstellungen High Anti-aliasing Type aus.',
    spyroHighTaa: 'Wähle in den Spieleinstellungen High TAA aus.',
    dieYoungTaa: 'Wähle in den Spieleinstellungen TAA „High“ oder „Epic“ aus.',
    kakarotBdzKfix:
      'Wähle in den Spieleinstellungen TAA aus und nutze BDZKFix für die Legacy-Version beziehungsweise dessen aktualisierten Fork für die HD-Version.',
    preyData: 'Bewahre die zusätzlichen Luma-Datendateien für Prey zusammen mit dem Add-on auf.',
    bundledDlss:
      'Luma sichert die mitgelieferte DLSS-Bibliothek des Spiels und stellt sie beim Entfernen wieder her.',
    daymareOptiscalerUuu:
      'Luma funktioniert allein, stürzt jedoch in Kombination mit OptiScaler oder UUU ab.',
    smtLyallFix: "Zum Erzwingen von TAA wird Lyall's Fix benötigt.",
    deusExBorisEnb:
      'Nicht kompatibel mit Boris ENB (DX9). Funktioniert mit DE und der Originalausgabe. Der Mod Gold Filter Restoration ist hier überflüssig.',
    xboxStore: 'Nicht kompatibel mit der Xbox-Store-Version.',
    massEffectNativeAa:
      'Nur DLAA / FSR 3 Native AA sind verfügbar; dies ist kein DLSS- oder FSR-Super-Resolution-Modus.',
    metaphorNativeAa:
      'Nur DLAA / FSR Native AA sind verfügbar; dies ist kein DLSS- oder FSR-Super-Resolution-Modus.',
    itTakesTwoTitle: 'Funktioniert nur während der Titelsequenz.',
    talesAriseSdk: 'Erfordert Arise-SDK mit UseUE4TAA = true.',
    metroWindowed:
      'Erfordert Fenster- oder rahmenlosen Fenstermodus, entweder über Mods oder durch Deaktivieren von Vollbild in der Spielkonfiguration.',
    edithFinch4k:
      'Das Spiel funktioniert in 4K nicht korrekt. Stelle Effects auf Low, bevor du Engine.ini manuell änderst.',
    sinkingCityOriginal: 'Die Originalversion funktioniert. Der Remaster-Status ist unbekannt.',
    heavyRainSteamUltrawide: 'Ultrawide funktioniert möglicherweise nur beim Start über Steam.',
    metroBorderless: 'Verwende den rahmenlosen Fenstermodus.',
  },
  es: {
    sherlockDx11Performance:
      'El argumento de inicio -dx11 puede reducir el rendimiento de la CPU en modo DX11. Con DLAA y AutoExposure:On pueden aparecer bordes dentados en la hierba.',
    guiltyGearStriveAa:
      'El antialiasing no funciona en la pantalla de selección de personaje. En el juego, elige AA «Temporal Anti Aliasing» y añade a Engine.ini: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
    manualLaunchArgument: 'Añade este argumento de inicio manualmente.',
    manualEngineIni: 'Aplica manualmente los siguientes ajustes en Engine.ini.',
    publicMatchmaking:
      'Evita el matchmaking público oficial mientras Luma esté instalado. Podría provocar un bloqueo.',
    edithFinchExit:
      'DLAA funciona sin cambios adicionales, pero es posible que el juego no se cierre por completo al salir. OptiScaler puede resolverlo.',
    dlssNoHdr: 'Solo DLSS (sin HDR por ahora).',
    kh3Txaa: 'Antes, selecciona «TXAA» en el juego.',
    aceFxaaHigh: 'Antes, selecciona AA «FXAA High» en el juego.',
    tetrisFxaa6: 'Antes, selecciona AA «FXAA:6» y escala de renderizado al 100 % en el juego.',
    projectWingmanFxaa: 'Antes, selecciona AA «FXAA» en el juego.',
    dnfCharacterSelection: 'El antialiasing no funciona en la pantalla de selección de personaje.',
    tekkenNoD3D9Ex: 'Como requisito, añade el argumento de inicio -nod3d9ex.',
    scornOptiscaler:
      'El juego tiene compatibilidad nativa con FSR 2.1, por lo que puedes añadir DLSS u otro escalador mediante OptiScaler.',
    hatsuneExclusiveFullscreen:
      'Si hay problemas, no uses pantalla completa exclusiva. Pulsa Alt+Enter para salir de ella.',
    deadlineUltra: 'En los ajustes del juego, selecciona «Ultra».',
    filamentAaHigh: 'En los ajustes del juego, selecciona AA «High» o «Very High».',
    aaHigh: 'En los ajustes del juego, selecciona AA «High».',
    mutantMotionBlur:
      'En los ajustes del juego, selecciona AA «High». Para una imagen en movimiento más clara, se recomienda r.motionblur.amount=0 en Engine.ini.',
    supralandTaa: 'En los ajustes del juego, selecciona AA «Temporal Anti Aliasing».',
    scarletNexusTxaa: 'En los ajustes del juego, selecciona AA «TXAA».',
    closeToSunAa4x: 'En los ajustes del juego, selecciona AA 4X.',
    darksidersAaEpic: 'En los ajustes del juego, selecciona AA Epic.',
    codeVeinAaHighest: 'En los ajustes del juego, selecciona AA Highest.',
    orcsAaHigh: 'En los ajustes del juego, selecciona la calidad de AA «High».',
    clashAaVeryHigh: 'En los ajustes del juego, selecciona la calidad de AA «Very High».',
    vampyrTxaa6x: 'En los ajustes del juego, selecciona AA TXAA 6X.',
    crashAaMedium:
      'En los ajustes del juego, selecciona al menos calidad de antialiasing Medium (2x).',
    callSeaEpic: 'En los ajustes del juego, selecciona calidad global «Epic».',
    spiritNorthUltra: 'En los ajustes del juego, selecciona calidad gráfica «Ultra».',
    goatHighAa: 'En los ajustes del juego, selecciona High AA.',
    crabHighAntialiasing: 'En los ajustes del juego, selecciona High Anti-aliasing Type.',
    spyroHighTaa: 'En los ajustes del juego, selecciona High TAA.',
    dieYoungTaa: 'En los ajustes del juego, selecciona TAA «High» o «Epic».',
    kakarotBdzKfix:
      'En los ajustes del juego, selecciona TAA y usa BDZKFix para la versión Legacy o su fork actualizado para la versión HD.',
    preyData: 'Mantén los archivos de datos adicionales de Luma para Prey junto al add-on.',
    bundledDlss:
      'Luma reserva la biblioteca DLSS incluida con el juego y la restaura al eliminarse.',
    daymareOptiscalerUuu:
      'Luma funciona por sí solo, pero se bloquea al combinarse con OptiScaler o UUU.',
    smtLyallFix: "Se necesita Lyall's Fix para forzar TAA.",
    deusExBorisEnb:
      'No es compatible con Boris ENB (DX9). Funciona con DE y la edición original. El mod Gold Filter Restoration es redundante.',
    xboxStore: 'No es compatible con la versión de Xbox Store.',
    massEffectNativeAa:
      'Solo están disponibles los modos DLAA / FSR 3 Native AA; no son superresolución DLSS ni FSR.',
    metaphorNativeAa:
      'Solo están disponibles los modos DLAA / FSR Native AA; no son superresolución DLSS ni FSR.',
    itTakesTwoTitle: 'Solo funciona durante la secuencia de la pantalla de título.',
    talesAriseSdk: 'Requiere Arise-SDK con UseUE4TAA = true.',
    metroWindowed:
      'Requiere modo ventana o ventana sin bordes, mediante mods o desactivando la pantalla completa en el archivo de configuración del juego.',
    edithFinch4k:
      'El juego no funciona correctamente en 4K. Ajusta Effects a Low antes de modificar Engine.ini manualmente.',
    sinkingCityOriginal:
      'La versión original funciona. Se desconoce el estado de la remasterización.',
    heavyRainSteamUltrawide:
      'El modo ultrapanorámico podría funcionar solo al iniciar mediante Steam.',
    metroBorderless: 'Usa el modo ventana sin bordes.',
  },
  fr: {
    sherlockDx11Performance:
      'L’argument de lancement -dx11 peut réduire les performances du processeur en mode DX11. Avec DLAA et AutoExposure:On, des bords crénelés peuvent apparaître sur l’herbe.',
    guiltyGearStriveAa:
      'L’anticrénelage ne fonctionne pas sur l’écran de sélection du personnage. Dans le jeu, choisissez AA «Temporal Anti Aliasing», puis ajoutez à Engine.ini : [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
    manualLaunchArgument: 'Ajoutez cet argument de lancement manuellement.',
    manualEngineIni: 'Appliquez manuellement les réglages suivants dans Engine.ini.',
    publicMatchmaking:
      'Évitez le matchmaking public officiel tant que Luma est installé. Cela peut entraîner un bannissement.',
    edithFinchExit:
      'DLAA fonctionne sans modification supplémentaire, mais le jeu peut ne pas se fermer complètement après avoir quitté. OptiScaler peut résoudre ce problème.',
    dlssNoHdr: 'DLSS uniquement (pas de HDR pour le moment).',
    kh3Txaa: 'Choisissez d’abord «TXAA» dans le jeu.',
    aceFxaaHigh: 'Choisissez d’abord AA «FXAA High» dans le jeu.',
    tetrisFxaa6: 'Choisissez d’abord AA «FXAA:6» et une échelle de rendu de 100 % dans le jeu.',
    projectWingmanFxaa: 'Choisissez d’abord AA «FXAA» dans le jeu.',
    dnfCharacterSelection:
      'L’anticrénelage ne fonctionne pas sur l’écran de sélection du personnage.',
    tekkenNoD3D9Ex: 'Comme prérequis, ajoutez l’argument de lancement -nod3d9ex.',
    scornOptiscaler:
      'Le jeu prend nativement en charge FSR 2.1 ; vous pouvez donc ajouter DLSS ou un autre upscaler via OptiScaler.',
    hatsuneExclusiveFullscreen:
      'En cas de problème, n’utilisez pas le plein écran exclusif. Appuyez sur Alt+Entrée pour le quitter.',
    deadlineUltra: 'Dans les réglages du jeu, choisissez «Ultra».',
    filamentAaHigh: 'Dans les réglages du jeu, choisissez AA «High» ou «Very High».',
    aaHigh: 'Dans les réglages du jeu, choisissez AA «High».',
    mutantMotionBlur:
      'Dans les réglages du jeu, choisissez AA «High». Pour des mouvements plus nets, r.motionblur.amount=0 est recommandé dans Engine.ini.',
    supralandTaa: 'Dans les réglages du jeu, choisissez AA «Temporal Anti Aliasing».',
    scarletNexusTxaa: 'Dans les réglages du jeu, choisissez AA «TXAA».',
    closeToSunAa4x: 'Dans les réglages du jeu, choisissez AA 4X.',
    darksidersAaEpic: 'Dans les réglages du jeu, choisissez AA Epic.',
    codeVeinAaHighest: 'Dans les réglages du jeu, choisissez AA Highest.',
    orcsAaHigh: 'Dans les réglages du jeu, choisissez la qualité AA «High».',
    clashAaVeryHigh: 'Dans les réglages du jeu, choisissez la qualité AA «Very High».',
    vampyrTxaa6x: 'Dans les réglages du jeu, choisissez AA TXAA 6X.',
    crashAaMedium:
      'Dans les réglages du jeu, choisissez au moins la qualité d’anticrénelage Medium (2x).',
    callSeaEpic: 'Dans les réglages du jeu, choisissez la qualité globale «Epic».',
    spiritNorthUltra: 'Dans les réglages du jeu, choisissez la qualité graphique «Ultra».',
    goatHighAa: 'Dans les réglages du jeu, choisissez High AA.',
    crabHighAntialiasing: 'Dans les réglages du jeu, choisissez High Anti-aliasing Type.',
    spyroHighTaa: 'Dans les réglages du jeu, choisissez High TAA.',
    dieYoungTaa: 'Dans les réglages du jeu, choisissez TAA «High» ou «Epic».',
    kakarotBdzKfix:
      'Dans les réglages du jeu, choisissez TAA et utilisez BDZKFix pour la version Legacy ou son fork mis à jour pour la version HD.',
    preyData: 'Conservez les fichiers de données Luma supplémentaires de Prey avec l’add-on.',
    bundledDlss:
      'Luma réserve la bibliothèque DLSS incluse avec le jeu et la restaure lors de la désinstallation.',
    daymareOptiscalerUuu:
      'Luma fonctionne seul, mais plante lorsqu’il est combiné avec OptiScaler ou UUU.',
    smtLyallFix: "Lyall's Fix est nécessaire pour forcer TAA.",
    deusExBorisEnb:
      'Incompatible avec Boris ENB (DX9). Fonctionne avec DE et l’édition originale. Le mod Gold Filter Restoration est redondant.',
    xboxStore: 'Incompatible avec la version Xbox Store.',
    massEffectNativeAa:
      'Seuls les modes DLAA / FSR 3 Native AA sont disponibles ; il ne s’agit pas de super-résolution DLSS ou FSR.',
    metaphorNativeAa:
      'Seuls les modes DLAA / FSR Native AA sont disponibles ; il ne s’agit pas de super-résolution DLSS ou FSR.',
    itTakesTwoTitle: 'Fonctionne uniquement pendant la séquence de l’écran-titre.',
    talesAriseSdk: 'Nécessite Arise-SDK avec UseUE4TAA = true.',
    metroWindowed:
      'Nécessite le mode fenêtré ou fenêtré sans bordure, via des mods ou en désactivant le plein écran dans le fichier de configuration du jeu.',
    edithFinch4k:
      'Le jeu ne fonctionne pas correctement en 4K. Réglez Effects sur Low avant de modifier Engine.ini manuellement.',
    sinkingCityOriginal:
      'La version originale fonctionne. L’état de la version remasterisée est inconnu.',
    heavyRainSteamUltrawide: 'L’ultralarge peut ne fonctionner qu’en lançant le jeu via Steam.',
    metroBorderless: 'Utilisez le mode fenêtré sans bordure.',
  },
  ja: {
    sherlockDx11Performance:
      '起動引数 -dx11 を指定すると、DX11 モードで CPU 性能が低下することがあります。AutoExposure:On で DLAA を使うと、草の縁にジャギーが出ることがあります。',
    guiltyGearStriveAa:
      'キャラクター選択画面ではアンチエイリアスが機能しません。ゲーム内で AA を「Temporal Anti Aliasing」に設定し、Engine.ini に次を追加してください: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4。',
    manualLaunchArgument: 'この起動引数を手動で追加してください。',
    manualEngineIni: '次の設定を Engine.ini に手動で適用してください。',
    publicMatchmaking:
      'Luma のインストール中は公式の公開マッチメイキングを利用しないでください。BAN される可能性があります。',
    edithFinchExit:
      'DLAA は追加の変更なしで動作しますが、終了後にゲームが完全に閉じないことがあります。OptiScaler で解決できる場合があります。',
    dlssNoHdr: 'DLSS のみ（現時点では HDR 非対応）。',
    kh3Txaa: '事前にゲーム内で「TXAA」を選択してください。',
    aceFxaaHigh: '事前にゲーム内で AA を「FXAA High」に選択してください。',
    tetrisFxaa6:
      '事前にゲーム内で AA を「FXAA:6」、レンダリングスケールを 100% に設定してください。',
    projectWingmanFxaa: '事前にゲーム内で AA を「FXAA」に選択してください。',
    dnfCharacterSelection: 'キャラクター選択画面ではアンチエイリアスが機能しません。',
    tekkenNoD3D9Ex: '前提条件として、起動引数 -nod3d9ex を追加してください。',
    scornOptiscaler:
      'ゲームは FSR 2.1 をネイティブでサポートしているため、OptiScaler で DLSS などのアップスケーラーを追加できます。',
    hatsuneExclusiveFullscreen:
      '不具合がある場合は排他的フルスクリーンを使用しないでください。Alt+Enter で解除できます。',
    deadlineUltra: 'ゲーム設定で「Ultra」を選択してください。',
    filamentAaHigh: 'ゲーム設定で AA を「High」または「Very High」にしてください。',
    aaHigh: 'ゲーム設定で AA を「High」にしてください。',
    mutantMotionBlur:
      'ゲーム設定で AA を「High」にしてください。動きを見やすくするには、Engine.ini で r.motionblur.amount=0 を設定することを推奨します。',
    supralandTaa: 'ゲーム設定で AA を「Temporal Anti Aliasing」にしてください。',
    scarletNexusTxaa: 'ゲーム設定で AA を「TXAA」にしてください。',
    closeToSunAa4x: 'ゲーム設定で AA を 4X にしてください。',
    darksidersAaEpic: 'ゲーム設定で AA を Epic にしてください。',
    codeVeinAaHighest: 'ゲーム設定で AA を Highest にしてください。',
    orcsAaHigh: 'ゲーム設定で AA 品質を「High」にしてください。',
    clashAaVeryHigh: 'ゲーム設定で AA 品質を「Very High」にしてください。',
    vampyrTxaa6x: 'ゲーム設定で AA を TXAA 6X にしてください。',
    crashAaMedium: 'ゲーム設定でアンチエイリアス品質を少なくとも Medium (2x) にしてください。',
    callSeaEpic: 'ゲーム設定で全体品質を「Epic」にしてください。',
    spiritNorthUltra: 'ゲーム設定でグラフィック品質を「Ultra」にしてください。',
    goatHighAa: 'ゲーム設定で High AA を選択してください。',
    crabHighAntialiasing: 'ゲーム設定で High Anti-aliasing Type を選択してください。',
    spyroHighTaa: 'ゲーム設定で High TAA を選択してください。',
    dieYoungTaa: 'ゲーム設定で TAA を「High」または「Epic」にしてください。',
    kakarotBdzKfix:
      'ゲーム設定で TAA を選択し、Legacy 版では BDZKFix、HD 版ではその更新フォークを使用してください。',
    preyData: 'Prey 用の追加 Luma データファイルは、アドオンと同じ場所に置いてください。',
    bundledDlss: 'Luma はゲームに同梱された DLSS ライブラリを退避し、削除時に復元します。',
    daymareOptiscalerUuu:
      'Luma は単体では動作しますが、OptiScaler または UUU と併用するとクラッシュします。',
    smtLyallFix: "TAA を強制するには Lyall's Fix が必要です。",
    deusExBorisEnb:
      'Boris ENB（DX9）とは互換性がありません。DE とオリジナル版で動作します。Gold Filter Restoration mod は不要です。',
    xboxStore: 'Xbox Store 版とは互換性がありません。',
    massEffectNativeAa:
      '利用できるのは DLAA / FSR 3 Native AA モードのみで、DLSS または FSR の超解像ではありません。',
    metaphorNativeAa:
      '利用できるのは DLAA / FSR Native AA モードのみで、DLSS または FSR の超解像ではありません。',
    itTakesTwoTitle: 'タイトル画面のシーケンスでのみ動作します。',
    talesAriseSdk: 'UseUE4TAA = true を設定した Arise-SDK が必要です。',
    metroWindowed:
      'ウィンドウまたはボーダーレスウィンドウモードが必要です。MOD を使うか、ゲーム設定ファイルでフルスクリーンを無効にしてください。',
    edithFinch4k:
      'ゲームは 4K で正常に動作しません。Engine.ini を手動で変更する前に Effects を Low に設定してください。',
    sinkingCityOriginal: 'オリジナル版は動作します。リマスター版の状況は不明です。',
    heavyRainSteamUltrawide:
      'ウルトラワイドは Steam 経由で起動した場合にのみ動作することがあります。',
    metroBorderless: 'ボーダーレスウィンドウモードを使用してください。',
  },
  zh: {
    sherlockDx11Performance:
      '启动参数 -dx11 可能降低 DX11 模式下的 CPU 性能。使用 DLAA 和 AutoExposure:On 时，草地边缘可能出现锯齿。',
    guiltyGearStriveAa:
      '角色选择界面中抗锯齿无效。在游戏内选择 AA「Temporal Anti Aliasing」，然后在 Engine.ini 中添加：[SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4。',
    manualLaunchArgument: '请手动添加此启动参数。',
    manualEngineIni: '请在 Engine.ini 中手动应用以下设置。',
    publicMatchmaking: '安装 Luma 后请避免使用官方公开匹配。这可能导致封禁。',
    edithFinchExit:
      'DLAA 无需额外修改即可工作，但退出后游戏可能无法完全关闭。OptiScaler 或可解决此问题。',
    dlssNoHdr: '仅支持 DLSS（暂不支持 HDR）。',
    kh3Txaa: '请先在游戏内选择「TXAA」。',
    aceFxaaHigh: '请先在游戏内选择 AA「FXAA High」。',
    tetrisFxaa6: '请先在游戏内选择 AA「FXAA:6」并将渲染比例设为 100%。',
    projectWingmanFxaa: '请先在游戏内选择 AA「FXAA」。',
    dnfCharacterSelection: '角色选择界面中抗锯齿无效。',
    tekkenNoD3D9Ex: '作为前置条件，请添加启动参数 -nod3d9ex。',
    scornOptiscaler: '游戏原生支持 FSR 2.1，因此可通过 OptiScaler 添加 DLSS 或其他超分辨率器。',
    hatsuneExclusiveFullscreen: '如出现问题，请不要使用独占全屏。按 Alt+Enter 可退出该模式。',
    deadlineUltra: '请在游戏设置中选择「Ultra」。',
    filamentAaHigh: '请在游戏设置中选择 AA「High」或「Very High」。',
    aaHigh: '请在游戏设置中选择 AA「High」。',
    mutantMotionBlur:
      '请在游戏设置中选择 AA「High」。为获得更清晰的运动画面，建议在 Engine.ini 中设置 r.motionblur.amount=0。',
    supralandTaa: '请在游戏设置中选择 AA「Temporal Anti Aliasing」。',
    scarletNexusTxaa: '请在游戏设置中选择 AA「TXAA」。',
    closeToSunAa4x: '请在游戏设置中选择 AA 4X。',
    darksidersAaEpic: '请在游戏设置中选择 AA Epic。',
    codeVeinAaHighest: '请在游戏设置中选择 AA Highest。',
    orcsAaHigh: '请在游戏设置中选择 AA 质量「High」。',
    clashAaVeryHigh: '请在游戏设置中选择 AA 质量「Very High」。',
    vampyrTxaa6x: '请在游戏设置中选择 AA TXAA 6X。',
    crashAaMedium: '请在游戏设置中至少选择 Medium (2x) 抗锯齿质量。',
    callSeaEpic: '请在游戏设置中选择全局质量「Epic」。',
    spiritNorthUltra: '请在游戏设置中选择图形质量「Ultra」。',
    goatHighAa: '请在游戏设置中选择 High AA。',
    crabHighAntialiasing: '请在游戏设置中选择 High Anti-aliasing Type。',
    spyroHighTaa: '请在游戏设置中选择 High TAA。',
    dieYoungTaa: '请在游戏设置中选择 TAA「High」或「Epic」。',
    kakarotBdzKfix: '请在游戏设置中选择 TAA；Legacy 版本使用 BDZKFix，HD 版本使用其更新分支。',
    preyData: '请将 Prey 的额外 Luma 数据文件与该插件放在一起。',
    bundledDlss: 'Luma 会备份游戏自带的 DLSS 库，并在移除时恢复。',
    daymareOptiscalerUuu: 'Luma 单独使用时正常，但与 OptiScaler 或 UUU 结合时会崩溃。',
    smtLyallFix: "强制启用 TAA 需要 Lyall's Fix。",
    deusExBorisEnb:
      '与 Boris ENB（DX9）不兼容。适用于 DE 和原版；Gold Filter Restoration mod 属于重复功能。',
    xboxStore: '与 Xbox Store 版本不兼容。',
    massEffectNativeAa: '仅提供 DLAA / FSR 3 Native AA 模式；这不是 DLSS 或 FSR 超分辨率。',
    metaphorNativeAa: '仅提供 DLAA / FSR Native AA 模式；这不是 DLSS 或 FSR 超分辨率。',
    itTakesTwoTitle: '仅在标题画面序列中有效。',
    talesAriseSdk: '需要 Arise-SDK，并设置 UseUE4TAA = true。',
    metroWindowed: '需要窗口或无边框窗口模式：可通过模组实现，或在游戏配置文件中关闭全屏。',
    edithFinch4k: '游戏在 4K 下无法正常工作。手动修改 Engine.ini 前，请将 Effects 设为 Low。',
    sinkingCityOriginal: '原版可用，重制版状态未知。',
    heavyRainSteamUltrawide: '超宽屏可能仅在通过 Steam 启动时可用。',
    metroBorderless: '请使用无边框窗口模式。',
  },
} satisfies Record<LumaGuidanceLocale, LumaGuidanceTranslations>;

export const lumaGuidanceKeys = Object.values(LUMA_GUIDANCE_GROUPS).flat();

function expandGuidanceTranslations(localized: LumaGuidanceTranslations): Record<string, string> {
  const overrides: Record<string, string> = {};

  for (const [phrase, ids] of Object.entries(LUMA_GUIDANCE_GROUPS) as [
    LumaGuidancePhrase,
    readonly string[],
  ][]) {
    for (const id of ids) {
      if (id in overrides) {
        throw new Error('Duplicate Luma guidance ID: ' + id);
      }

      overrides[id] = localized[phrase];
    }
  }

  return overrides;
}

/**
 * Dynamic catalog overrides. English is intentionally absent so newly added
 * manifest IDs and any untranslated entries keep their reviewed fallback text.
 */
export const lumaGuidanceOverrides: Partial<Record<Locale, Record<string, string>>> =
  Object.fromEntries(
    Object.entries(translations).map(([locale, localized]) => [
      locale,
      expandGuidanceTranslations(localized),
    ]),
  );
