import { defineLocalizedOverrides } from '../../contract';
import {
  expandLumaGuidanceTranslations,
  type LumaGuidanceTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
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
} as const satisfies LumaGuidanceTranslations;

export const lumaGuidanceOverrides = defineLocalizedOverrides<'ru', LumaSourceCatalog>()(
  expandLumaGuidanceTranslations(translations),
);
