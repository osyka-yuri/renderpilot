import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'Predefinição de renderização',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    'Força uma predefinição específica do DLSS Super Resolution. Alguns jogos também podem exigir o “Perfil de predefinição de modelo forçado” para aplicar predefinições personalizadas.',
  'Off (game default)': 'Desativado (padrão do jogo)',
  'Preset A (CNN)': 'Predefinição A (CNN)',
  'Preset B (CNN)': 'Predefinição B (CNN)',
  'Preset C (CNN)': 'Predefinição C (CNN)',
  'Preset D (CNN)': 'Predefinição D (CNN)',
  'Preset E (CNN)': 'Predefinição E (CNN)',
  'Preset F (CNN)': 'Predefinição F (CNN)',
  'Preset G (unused)': 'Predefinição G (não utilizada)',
  'Preset H (unused)': 'Predefinição H (não utilizada)',
  'Preset I (unused)': 'Predefinição I (não utilizada)',
  'Preset J (Transformer Gen 1)': 'Predefinição J (Transformer Gen 1)',
  'Preset K (Transformer Gen 1)': 'Predefinição K (Transformer Gen 1)',
  'Preset L (Transformer Gen 2)': 'Predefinição L (Transformer Gen 2)',
  'Preset M (Transformer Gen 2)': 'Predefinição M (Transformer Gen 2)',
  'Preset N (unused)': 'Predefinição N (não utilizada)',
  'Preset O (unused)': 'Predefinição O (não utilizada)',
  Recommended: 'Recomendado',
  'Forced Quality Level': 'Nível de qualidade forçado',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'Substitui a seleção de qualidade do DLSS Super Resolution feita no jogo.',
  Performance: 'Desempenho',
  Balanced: 'Equilibrado',
  Quality: 'Qualidade',
  'N/A': 'Não disponível',
  'Ultra Performance': 'Desempenho ultra',
  Custom: 'Personalizado',
  'Forced Scaling Ratio': 'Proporção de escalonamento forçada',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'Define uma proporção personalizada da resolução de renderização. O “Nível de qualidade forçado” precisa estar definido como “Personalizado”.',
  Off: 'Desativado',
  '33% (Ultra Performance)': '33% (desempenho ultra)',
  '50% (Performance)': '50% (desempenho)',
  '58% (Balanced)': '58% (equilibrado)',
  '67% (Quality)': '67% (qualidade)',
  '77% (Ultra Quality)': '77% (qualidade ultra)',
  'Enable DLL Override': 'Ativar substituição de DLL',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'Força o jogo a usar a versão mais recente do DLSS-SR instalada em todo o sistema. Compatível com a maioria dos títulos com DLSS 2 ou posterior.',
  'On (use latest installed)': 'Ativado (usar a versão instalada mais recente)',
  'Forced Model Preset Profile': 'Perfil de predefinição de modelo forçado',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    'Permite aplicar uma predefinição personalizada em jogos nos quais a “Predefinição de renderização” não tem efeito por padrão.',
  'Force DLAA (full-resolution)': 'Forçar DLAA (resolução completa)',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'Renderiza todos os modos de qualidade do DLSS em resolução completa, funcionando como uma solução de antisserrilhamento (DLAA).',
  On: 'Ativado',
  'Remap Performance to Ultra Performance': 'Remapear Desempenho para Desempenho ultra',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    'Força o modo de qualidade “Desempenho” a usar o caminho de renderização “Desempenho ultra”.',
  'Forces a specific DLSS Frame Generation preset.':
    'Força uma predefinição específica do DLSS Frame Generation.',
  'Preset A': 'Predefinição A',
  'Preset B': 'Predefinição B',
  'Preset C (unused)': 'Predefinição C (não utilizada)',
  'Preset D (unused)': 'Predefinição D (não utilizada)',
  'Preset E (unused)': 'Predefinição E (não utilizada)',
  'Preset F (unused)': 'Predefinição F (não utilizada)',
  'Preset J (unused)': 'Predefinição J (não utilizada)',
  'Preset K (unused)': 'Predefinição K (não utilizada)',
  'Preset L (unused)': 'Predefinição L (não utilizada)',
  'Preset M (unused)': 'Predefinição M (não utilizada)',
  'Forced Mode': 'Modo forçado',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'Define o modo do Frame Generation. O modo dinâmico exige o driver 595.97 ou mais recente.',
  Fixed: 'Fixo',
  Dynamic: 'Dinâmico',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'Força o jogo a usar a versão mais recente do DLSS-FG instalada em todo o sistema. Compatível com a maioria dos títulos com DLSS 3.',
  'Multi-Frame Generation — Fixed Count': 'Multi Frame Generation — quantidade fixa',
  'Sets a fixed number of generated frames per rendered frame.':
    'Define uma quantidade fixa de quadros gerados para cada quadro renderizado.',
  'Multi-Frame Generation — Dynamic Count': 'Multi Frame Generation — quantidade dinâmica',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'Define o limite máximo de quadros gerados quando o Frame Generation está no modo dinâmico.',
  'Up to 2x': 'Até 2x',
  'Up to 3x': 'Até 3x',
  'Up to 4x': 'Até 4x',
  'Up to 5x': 'Até 5x',
  'Up to 6x': 'Até 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'Multi Frame Generation — taxa de quadros dinâmica desejada',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'Define a taxa de quadros que o Frame Generation dinâmico tentará manter.',
  'Max Refresh Rate': 'Taxa de atualização máxima',
  'Forces a specific DLSS Ray Reconstruction preset.':
    'Força uma predefinição específica do DLSS Ray Reconstruction.',
  'Preset D (Transformer Gen 1)': 'Predefinição D (Transformer Gen 1)',
  'Preset E (Transformer Gen 1)': 'Predefinição E (Transformer Gen 1)',
  'Preset F (Transformer Gen 2)': 'Predefinição F (Transformer Gen 2)',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'Substitui a seleção de qualidade do DLSS Ray Reconstruction feita no jogo.',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'Força o jogo a usar a versão mais recente do DLSS-RR instalada em todo o sistema. Compatível com a maioria dos títulos com Ray Reconstruction.',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'pt-BR', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
