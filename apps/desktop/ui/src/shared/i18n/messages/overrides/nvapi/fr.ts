import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'Préréglage de rendu',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    'Force un préréglage spécifique de DLSS Super Résolution. Certains jeux peuvent aussi nécessiter « Profil de préréglage de modèle forcé » pour appliquer des préréglages personnalisés.',
  'Off (game default)': 'Désactivé (valeur du jeu)',
  'Preset A (CNN)': 'Préréglage A (CNN)',
  'Preset B (CNN)': 'Préréglage B (CNN)',
  'Preset C (CNN)': 'Préréglage C (CNN)',
  'Preset D (CNN)': 'Préréglage D (CNN)',
  'Preset E (CNN)': 'Préréglage E (CNN)',
  'Preset F (CNN)': 'Préréglage F (CNN)',
  'Preset G (unused)': 'Préréglage G (inutilisé)',
  'Preset H (unused)': 'Préréglage H (inutilisé)',
  'Preset I (unused)': 'Préréglage I (inutilisé)',
  'Preset J (Transformer Gen 1)': 'Préréglage J (Transformer Gen 1)',
  'Preset K (Transformer Gen 1)': 'Préréglage K (Transformer Gen 1)',
  'Preset L (Transformer Gen 2)': 'Préréglage L (Transformer Gen 2)',
  'Preset M (Transformer Gen 2)': 'Préréglage M (Transformer Gen 2)',
  'Preset N (unused)': 'Préréglage N (inutilisé)',
  'Preset O (unused)': 'Préréglage O (inutilisé)',
  Recommended: 'Recommandé',
  'Forced Quality Level': 'Niveau de qualité forcé',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'Remplace le niveau de qualité de DLSS Super Résolution sélectionné dans le jeu.',
  Performance: 'Performances',
  Balanced: 'Équilibré',
  Quality: 'Qualité',
  'N/A': 'N/D',
  'Ultra Performance': 'Ultra performances',
  Custom: 'Personnalisé',
  'Forced Scaling Ratio': 'Rapport de mise à l’échelle forcé',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'Définit un rapport personnalisé de résolution de rendu. « Niveau de qualité forcé » doit être réglé sur « Personnalisé ».',
  Off: 'Désactivé',
  '33% (Ultra Performance)': '33% (ultra performances)',
  '50% (Performance)': '50% (performances)',
  '58% (Balanced)': '58% (équilibré)',
  '67% (Quality)': '67% (qualité)',
  '77% (Ultra Quality)': '77% (ultra qualité)',
  'Enable DLL Override': 'Activer le remplacement de DLL',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'Force le jeu à utiliser la dernière version de DLSS-SR installée sur le système. Compatible avec la plupart des titres DLSS 2 ou ultérieurs.',
  'On (use latest installed)': 'Activé (utiliser la dernière version installée)',
  'Forced Model Preset Profile': 'Profil de préréglage de modèle forcé',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    'Permet d’appliquer un préréglage personnalisé dans les jeux où « Préréglage de rendu » est sans effet par défaut.',
  'Force DLAA (full-resolution)': 'Forcer DLAA (pleine résolution)',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'Effectue le rendu de chaque mode de qualité DLSS en pleine résolution et agit comme solution d’anticrénelage (DLAA).',
  On: 'Activé',
  'Remap Performance to Ultra Performance': 'Remapper Performances vers Ultra performances',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    'Force le mode de qualité « Performances » à utiliser le chemin de rendu « Ultra performances ».',
  'Forces a specific DLSS Frame Generation preset.':
    'Force un préréglage spécifique de DLSS Génération d’images.',
  'Preset A': 'Préréglage A',
  'Preset B': 'Préréglage B',
  'Preset C (unused)': 'Préréglage C (inutilisé)',
  'Preset D (unused)': 'Préréglage D (inutilisé)',
  'Preset E (unused)': 'Préréglage E (inutilisé)',
  'Preset F (unused)': 'Préréglage F (inutilisé)',
  'Preset J (unused)': 'Préréglage J (inutilisé)',
  'Preset K (unused)': 'Préréglage K (inutilisé)',
  'Preset L (unused)': 'Préréglage L (inutilisé)',
  'Preset M (unused)': 'Préréglage M (inutilisé)',
  'Forced Mode': 'Mode forcé',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'Définit le mode de Génération d’images. Le mode dynamique nécessite le pilote 595.97 ou ultérieur.',
  Fixed: 'Fixe',
  Dynamic: 'Dynamique',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'Force le jeu à utiliser la dernière version de DLSS-FG installée sur le système. Compatible avec la plupart des titres DLSS 3.',
  'Multi-Frame Generation — Fixed Count': 'Génération multi-images — nombre fixe',
  'Sets a fixed number of generated frames per rendered frame.':
    'Définit un nombre fixe d’images générées pour chaque image rendue.',
  'Multi-Frame Generation — Dynamic Count': 'Génération multi-images — nombre dynamique',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'Définit la limite supérieure des images générées lorsque la Génération d’images est en mode dynamique.',
  'Up to 2x': 'Jusqu’à 2x',
  'Up to 3x': 'Jusqu’à 3x',
  'Up to 4x': 'Jusqu’à 4x',
  'Up to 5x': 'Jusqu’à 5x',
  'Up to 6x': 'Jusqu’à 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'Génération multi-images — fréquence dynamique cible',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'Définit la fréquence d’images cible que la Génération d’images dynamique cherche à maintenir.',
  'Max Refresh Rate': 'Fréquence de rafraîchissement maximale',
  'Forces a specific DLSS Ray Reconstruction preset.':
    'Force un préréglage spécifique de DLSS Reconstruction de rayons.',
  'Preset D (Transformer Gen 1)': 'Préréglage D (Transformer Gen 1)',
  'Preset E (Transformer Gen 1)': 'Préréglage E (Transformer Gen 1)',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'Remplace le niveau de qualité de DLSS Reconstruction de rayons sélectionné dans le jeu.',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'Force le jeu à utiliser la dernière version de DLSS-RR installée sur le système. Compatible avec la plupart des titres avec Reconstruction de rayons.',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'fr', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
