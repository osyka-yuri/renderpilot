import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'Preajuste de renderizado',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    'Fuerza un preajuste específico de DLSS Superresolución. Algunos juegos también pueden requerir «Perfil de preajuste de modelo forzado» para aplicar preajustes personalizados.',
  'Off (game default)': 'Desactivado (valor del juego)',
  'Preset A (CNN)': 'Preajuste A (CNN)',
  'Preset B (CNN)': 'Preajuste B (CNN)',
  'Preset C (CNN)': 'Preajuste C (CNN)',
  'Preset D (CNN)': 'Preajuste D (CNN)',
  'Preset E (CNN)': 'Preajuste E (CNN)',
  'Preset F (CNN)': 'Preajuste F (CNN)',
  'Preset G (unused)': 'Preajuste G (no utilizado)',
  'Preset H (unused)': 'Preajuste H (no utilizado)',
  'Preset I (unused)': 'Preajuste I (no utilizado)',
  'Preset J (Transformer Gen 1)': 'Preajuste J (Transformer Gen 1)',
  'Preset K (Transformer Gen 1)': 'Preajuste K (Transformer Gen 1)',
  'Preset L (Transformer Gen 2)': 'Preajuste L (Transformer Gen 2)',
  'Preset M (Transformer Gen 2)': 'Preajuste M (Transformer Gen 2)',
  'Preset N (unused)': 'Preajuste N (no utilizado)',
  'Preset O (unused)': 'Preajuste O (no utilizado)',
  Recommended: 'Recomendado',
  'Forced Quality Level': 'Nivel de calidad forzado',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'Anula el nivel de calidad de DLSS Superresolución seleccionado en el juego.',
  Performance: 'Rendimiento',
  Balanced: 'Equilibrado',
  Quality: 'Calidad',
  'N/A': 'No disponible',
  'Ultra Performance': 'Rendimiento ultra',
  Custom: 'Personalizado',
  'Forced Scaling Ratio': 'Relación de escalado forzada',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'Establece una relación personalizada de resolución de renderizado. «Nivel de calidad forzado» debe estar en «Personalizado».',
  Off: 'Desactivado',
  '33% (Ultra Performance)': '33% (rendimiento ultra)',
  '50% (Performance)': '50% (rendimiento)',
  '58% (Balanced)': '58% (equilibrado)',
  '67% (Quality)': '67% (calidad)',
  '77% (Ultra Quality)': '77% (calidad ultra)',
  'Enable DLL Override': 'Activar sustitución de DLL',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'Fuerza al juego a usar la versión más reciente de DLSS-SR instalada en el sistema. Compatible con la mayoría de títulos con DLSS 2 o posterior.',
  'On (use latest installed)': 'Activado (usar la última versión instalada)',
  'Forced Model Preset Profile': 'Perfil de preajuste de modelo forzado',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    'Permite aplicar un preajuste personalizado en juegos donde «Preajuste de renderizado» no tiene efecto de forma predeterminada.',
  'Force DLAA (full-resolution)': 'Forzar DLAA (resolución completa)',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'Renderiza todos los modos de calidad de DLSS a resolución completa y actúa como solución de antialiasing (DLAA).',
  On: 'Activado',
  'Remap Performance to Ultra Performance': 'Reasignar Rendimiento a Rendimiento ultra',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    'Fuerza al modo de calidad «Rendimiento» a usar la ruta de renderizado «Rendimiento ultra».',
  'Forces a specific DLSS Frame Generation preset.':
    'Fuerza un preajuste específico de DLSS Generación de fotogramas.',
  'Preset A': 'Preajuste A',
  'Preset B': 'Preajuste B',
  'Preset C (unused)': 'Preajuste C (no utilizado)',
  'Preset D (unused)': 'Preajuste D (no utilizado)',
  'Preset E (unused)': 'Preajuste E (no utilizado)',
  'Preset F (unused)': 'Preajuste F (no utilizado)',
  'Preset J (unused)': 'Preajuste J (no utilizado)',
  'Preset K (unused)': 'Preajuste K (no utilizado)',
  'Preset L (unused)': 'Preajuste L (no utilizado)',
  'Preset M (unused)': 'Preajuste M (no utilizado)',
  'Forced Mode': 'Modo forzado',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'Establece el modo de Generación de fotogramas. El modo dinámico requiere el controlador 595.97 o posterior.',
  Fixed: 'Fijo',
  Dynamic: 'Dinámico',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'Fuerza al juego a usar la versión más reciente de DLSS-FG instalada en el sistema. Compatible con la mayoría de títulos con DLSS 3.',
  'Multi-Frame Generation — Fixed Count': 'Generación de fotogramas múltiples — cantidad fija',
  'Sets a fixed number of generated frames per rendered frame.':
    'Establece una cantidad fija de fotogramas generados por cada fotograma renderizado.',
  'Multi-Frame Generation — Dynamic Count':
    'Generación de fotogramas múltiples — cantidad dinámica',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'Establece el límite máximo de fotogramas generados cuando la Generación de fotogramas está en modo dinámico.',
  'Up to 2x': 'Hasta 2x',
  'Up to 3x': 'Hasta 3x',
  'Up to 4x': 'Hasta 4x',
  'Up to 5x': 'Hasta 5x',
  'Up to 6x': 'Hasta 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'Generación de fotogramas múltiples — tasa dinámica objetivo',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'Establece la tasa de fotogramas objetivo que la Generación de fotogramas dinámica intenta mantener.',
  'Max Refresh Rate': 'Frecuencia de actualización máxima',
  'Forces a specific DLSS Ray Reconstruction preset.':
    'Fuerza un preajuste específico de DLSS Reconstrucción de rayos.',
  'Preset D (Transformer Gen 1)': 'Preajuste D (Transformer Gen 1)',
  'Preset E (Transformer Gen 1)': 'Preajuste E (Transformer Gen 1)',
  'Preset F (Transformer Gen 2)': 'Preajuste F (Transformer Gen 2)',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'Anula el nivel de calidad de DLSS Reconstrucción de rayos seleccionado en el juego.',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'Fuerza al juego a usar la versión más reciente de DLSS-RR instalada en el sistema. Compatible con la mayoría de títulos con Reconstrucción de rayos.',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'es', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
