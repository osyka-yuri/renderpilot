import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
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
  fallout4DlssGtaoOnly: 'Actualmente, este perfil solo admite DLSS y GTAO.',
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
  aaUltra: 'En los ajustes del juego, selecciona AA «Ultra».',
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
  dlssOnlyNoHdr:
    'Este perfil solo añade compatibilidad con DLSS; HDR no es compatible actualmente.',
  biomutantAaHighOrMax: 'En los ajustes del juego, usa AA «High» o «Max».',
  blairWitchTxaaFull: 'En los ajustes del juego, usa TXAA y escala de resolución «Full».',
  flickeringIssues: 'Parece haber problemas de parpadeo.',
  brambleEpicVram:
    'La calidad Epic puede llenar progresivamente la VRAM y provocar tirones. Evita cambiar repetidamente entre High y Epic mientras Luma esté activo.',
  daemonDlaaReset:
    'Cargar un nivel o cambiar los ajustes gráficos fuerza r.TemporalAASamples=1 y desactiva DLAA.',
  easyAntiCheatBlocked: 'Bloqueado por Easy Anti-Cheat.',
  echoDlaaAutoExposure:
    'DLAA deja de funcionar tras el primer nivel. Con AutoExposure: On, las fuentes de luz parpadean; con AutoExposure: Off, el antialiasing empeora mucho.',
  dx11BootFailure: 'No se inicia con DX11.',
  rainCodeAaHighMaxResolution:
    'En los ajustes del juego, usa calidad de AA «High» y pon el control de resolución al máximo.',
  roboquestTaaQuality3: 'En los ajustes del juego, usa TAA y calidad «3».',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'es', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
