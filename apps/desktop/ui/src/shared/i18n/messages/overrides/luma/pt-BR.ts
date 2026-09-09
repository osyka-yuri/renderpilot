import { defineLocalizedCatalog } from '../../contract';
import {
  expandLumaTranslations,
  type LumaMessageTranslations,
  type LumaSourceCatalog,
} from './schema';

const translations = {
  sherlockDx11Performance:
    'O argumento de inicialização -dx11 pode reduzir o desempenho da CPU no modo DX11. Com DLAA e AutoExposure:On, podem aparecer bordas serrilhadas na grama.',
  guiltyGearStriveAa:
    'O antisserrilhamento não funciona na tela de seleção de personagens. No jogo, escolha o AA “Temporal Anti Aliasing” e adicione ao Engine.ini: [SystemSettings] r.DefaultFeature.AntiAliasing=2 r.PostProcessAAQuality=4.',
  manualLaunchArgument: 'Adicione este argumento de inicialização manualmente.',
  manualEngineIni: 'Aplique manualmente as seguintes configurações no Engine.ini.',
  publicMatchmaking:
    'Evite o pareamento público oficial enquanto o Luma estiver instalado. Isso pode causar um banimento.',
  edithFinchExit:
    'O DLAA funciona sem alterações adicionais, mas o jogo pode não fechar completamente ao sair. O OptiScaler pode resolver isso.',
  dlssNoHdr: 'Somente DLSS (sem HDR por enquanto).',
  fallout4DlssGtaoOnly: 'Atualmente, este perfil oferece suporte apenas a DLSS e GTAO.',
  kh3Txaa: 'Primeiro, selecione “TXAA” no jogo.',
  aceFxaaHigh: 'Primeiro, selecione o AA “FXAA High” no jogo.',
  tetrisFxaa6: 'Primeiro, selecione o AA “FXAA:6” e a escala de renderização em 100% no jogo.',
  projectWingmanFxaa: 'Primeiro, selecione o AA “FXAA” no jogo.',
  dnfCharacterSelection: 'O antisserrilhamento não funciona na tela de seleção de personagens.',
  tekkenNoD3D9Ex: 'Como requisito, adicione o argumento de inicialização -nod3d9ex.',
  scornOptiscaler:
    'O jogo oferece suporte nativo ao FSR 2.1, portanto você pode adicionar DLSS ou outro escalonador com o OptiScaler.',
  hatsuneExclusiveFullscreen:
    'Se ocorrerem problemas, não use tela cheia exclusiva. Pressione Alt+Enter para sair dela.',
  deadlineUltra: 'Nas configurações do jogo, selecione “Ultra”.',
  filamentAaHigh: 'Nas configurações do jogo, selecione o AA “High” ou “Very High”.',
  aaHigh: 'Nas configurações do jogo, selecione o AA “High”.',
  aaUltra: 'Nas configurações do jogo, selecione o AA “Ultra”.',
  mutantMotionBlur:
    'Nas configurações do jogo, selecione o AA “High”. Para uma imagem em movimento mais nítida, recomenda-se r.motionblur.amount=0 no Engine.ini.',
  supralandTaa: 'Nas configurações do jogo, selecione o AA “Temporal Anti Aliasing”.',
  scarletNexusTxaa: 'Nas configurações do jogo, selecione o AA “TXAA”.',
  closeToSunAa4x: 'Nas configurações do jogo, selecione AA 4X.',
  darksidersAaEpic: 'Nas configurações do jogo, selecione AA Epic.',
  codeVeinAaHighest: 'Nas configurações do jogo, selecione AA Highest.',
  orcsAaHigh: 'Nas configurações do jogo, selecione a qualidade de AA “High”.',
  clashAaVeryHigh: 'Nas configurações do jogo, selecione a qualidade de AA “Very High”.',
  vampyrTxaa6x: 'Nas configurações do jogo, selecione AA TXAA 6X.',
  crashAaMedium:
    'Nas configurações do jogo, selecione pelo menos a qualidade de antisserrilhamento Medium (2x).',
  callSeaEpic: 'Nas configurações do jogo, selecione a qualidade geral “Epic”.',
  spiritNorthUltra: 'Nas configurações do jogo, selecione a qualidade gráfica “Ultra”.',
  goatHighAa: 'Nas configurações do jogo, selecione High AA.',
  crabHighAntialiasing: 'Nas configurações do jogo, selecione High Anti-aliasing Type.',
  spyroHighTaa: 'Nas configurações do jogo, selecione High TAA.',
  dieYoungTaa: 'Nas configurações do jogo, selecione TAA “High” ou “Epic”.',
  kakarotBdzKfix:
    'Nas configurações do jogo, selecione TAA e use o BDZKFix para a versão Legacy ou o fork atualizado dele para a versão HD.',
  preyData: 'Mantenha os arquivos de dados adicionais do Luma para Prey junto ao complemento.',
  daymareOptiscalerUuu:
    'O Luma funciona sozinho, mas trava quando combinado com o OptiScaler ou o UUU.',
  smtLyallFix: "O Lyall's Fix é necessário para forçar o TAA.",
  deusExBorisEnb:
    'Não é compatível com o Boris ENB (DX9). Funciona com DE e a edição original. O mod Gold Filter Restoration é redundante.',
  xboxStore: 'Não é compatível com a versão da Xbox Store.',
  massEffectNativeAa:
    'Somente os modos DLAA/FSR 3 Native AA estão disponíveis; eles não são Super-resolução DLSS nem FSR.',
  metaphorNativeAa:
    'Somente os modos DLAA/FSR Native AA estão disponíveis; eles não são Super-resolução DLSS nem FSR.',
  itTakesTwoTitle: 'Funciona somente durante a sequência da tela de título.',
  talesAriseSdk: 'Exige o Arise-SDK com UseUE4TAA = true.',
  metroWindowed:
    'Exige o modo janela ou janela sem bordas, usando mods ou desativando a tela cheia no arquivo de configuração do jogo.',
  edithFinch4k:
    'O jogo não funciona corretamente em 4K. Defina Effects como Low antes de modificar o Engine.ini manualmente.',
  sinkingCityOriginal: 'A versão original funciona. O estado da remasterização é desconhecido.',
  heavyRainSteamUltrawide:
    'O modo ultrawide pode funcionar somente quando o jogo é iniciado pelo Steam.',
  metroBorderless: 'Use o modo janela sem bordas.',
  dlssOnlyNoHdr: 'Este perfil adiciona somente suporte ao DLSS; o HDR não é compatível no momento.',
  biomutantAaHighOrMax: 'Nas configurações do jogo, defina o AA como “High” ou “Max”.',
  blairWitchTxaaFull: 'Nas configurações do jogo, use TXAA e a escala de resolução “Full”.',
  flickeringIssues: 'Parece haver problemas de cintilação.',
  brambleEpicVram:
    'A qualidade Epic pode encher progressivamente a VRAM e causar travamentos. Evite alternar repetidamente entre High e Epic enquanto o Luma estiver ativo.',
  daemonDlaaReset:
    'Carregar uma fase ou alterar as configurações gráficas força r.TemporalAASamples=1 e desativa o DLAA.',
  easyAntiCheatBlocked: 'Bloqueado por Easy Anti-Cheat.',
  echoDlaaAutoExposure:
    'O DLAA para de funcionar após a primeira fase. Com AutoExposure: On, as fontes de luz piscam; com AutoExposure: Off, o antisserrilhamento piora muito.',
  dx11BootFailure: 'Não inicia com DX11.',
  rainCodeAaHighMaxResolution:
    'Nas configurações do jogo, use a qualidade de AA “High” e coloque o controle de resolução no máximo.',
  roboquestTaaQuality3: 'Nas configurações do jogo, use TAA e qualidade “3”.',
} as const satisfies LumaMessageTranslations;

export const lumaOverrides = defineLocalizedCatalog<'pt-BR', LumaSourceCatalog>()(
  expandLumaTranslations(translations),
);
