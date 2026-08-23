import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'Rendering-Preset',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    'Erzwingt ein bestimmtes Preset für DLSS Superhohe Auflösung. In manchen Spielen ist zusätzlich „Erzwungenes Modell-Preset-Profil“ erforderlich, um benutzerdefinierte Presets anzuwenden.',
  'Off (game default)': 'Aus (Spielstandard)',
  'Preset A (CNN)': 'Voreinstellung A (CNN)',
  'Preset B (CNN)': 'Voreinstellung B (CNN)',
  'Preset C (CNN)': 'Voreinstellung C (CNN)',
  'Preset D (CNN)': 'Voreinstellung D (CNN)',
  'Preset E (CNN)': 'Voreinstellung E (CNN)',
  'Preset F (CNN)': 'Voreinstellung F (CNN)',
  'Preset G (unused)': 'Voreinstellung G (nicht verwendet)',
  'Preset H (unused)': 'Voreinstellung H (nicht verwendet)',
  'Preset I (unused)': 'Voreinstellung I (nicht verwendet)',
  'Preset J (Transformer Gen 1)': 'Voreinstellung J (Transformer Gen 1)',
  'Preset K (Transformer Gen 1)': 'Voreinstellung K (Transformer Gen 1)',
  'Preset L (Transformer Gen 2)': 'Voreinstellung L (Transformer Gen 2)',
  'Preset M (Transformer Gen 2)': 'Voreinstellung M (Transformer Gen 2)',
  'Preset N (unused)': 'Voreinstellung N (nicht verwendet)',
  'Preset O (unused)': 'Voreinstellung O (nicht verwendet)',
  Recommended: 'Empfohlen',
  'Forced Quality Level': 'Erzwungene Qualitätsstufe',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'Überschreibt die im Spiel ausgewählte Qualitätsstufe für DLSS Superhohe Auflösung.',
  Performance: 'Leistung',
  Balanced: 'Ausgeglichen',
  Quality: 'Qualität',
  'N/A': 'k. A.',
  'Ultra Performance': 'Ultra-Leistung',
  Custom: 'Benutzerdefiniert',
  'Forced Scaling Ratio': 'Erzwungenes Skalierungsverhältnis',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'Legt ein benutzerdefiniertes Verhältnis für die Rendering-Auflösung fest. „Erzwungene Qualitätsstufe“ muss auf „Benutzerdefiniert“ gesetzt sein.',
  Off: 'Aus',
  '33% (Ultra Performance)': '33% (Ultra-Leistung)',
  '50% (Performance)': '50% (Leistung)',
  '58% (Balanced)': '58% (Ausgeglichen)',
  '67% (Quality)': '67% (Qualität)',
  '77% (Ultra Quality)': '77% (Ultra-Qualität)',
  'Enable DLL Override': 'DLL-Überschreibung aktivieren',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'Erzwingt im Spiel die neueste systemweit installierte DLSS-SR-Version. Wird von den meisten Titeln mit DLSS 2 oder neuer unterstützt.',
  'On (use latest installed)': 'Ein (neueste installierte Version verwenden)',
  'Forced Model Preset Profile': 'Erzwungenes Modell-Preset-Profil',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    'Ermöglicht ein benutzerdefiniertes Preset in Spielen, in denen „Rendering-Preset“ standardmäßig keine Wirkung hat.',
  'Force DLAA (full-resolution)': 'DLAA erzwingen (volle Auflösung)',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'Rendert jeden DLSS-Qualitätsmodus in voller Auflösung und verwendet ihn als Kantenglättungslösung (DLAA).',
  On: 'Ein',
  'Remap Performance to Ultra Performance': 'Leistung auf Ultra-Leistung umleiten',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    'Erzwingt für den Qualitätsmodus „Leistung“ den Rendering-Pfad „Ultra-Leistung“.',
  'Forces a specific DLSS Frame Generation preset.':
    'Erzwingt ein bestimmtes Preset für die DLSS Frame-Erstellung.',
  'Preset A': 'Voreinstellung A',
  'Preset B': 'Voreinstellung B',
  'Preset C (unused)': 'Voreinstellung C (nicht verwendet)',
  'Preset D (unused)': 'Voreinstellung D (nicht verwendet)',
  'Preset E (unused)': 'Voreinstellung E (nicht verwendet)',
  'Preset F (unused)': 'Voreinstellung F (nicht verwendet)',
  'Preset J (unused)': 'Voreinstellung J (nicht verwendet)',
  'Preset K (unused)': 'Voreinstellung K (nicht verwendet)',
  'Preset L (unused)': 'Voreinstellung L (nicht verwendet)',
  'Preset M (unused)': 'Voreinstellung M (nicht verwendet)',
  'Forced Mode': 'Erzwungener Modus',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'Legt den Modus der Frame-Erstellung fest. Der dynamische Modus benötigt Treiber 595.97 oder neuer.',
  Fixed: 'Fest',
  Dynamic: 'Dynamisch',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'Erzwingt im Spiel die neueste systemweit installierte DLSS-FG-Version. Wird von den meisten DLSS-3-Titeln unterstützt.',
  'Multi-Frame Generation — Fixed Count': 'Multi Frame Generation — feste Anzahl',
  'Sets a fixed number of generated frames per rendered frame.':
    'Legt eine feste Anzahl erzeugter Frames pro gerendertem Frame fest.',
  'Multi-Frame Generation — Dynamic Count': 'Multi Frame Generation — dynamische Anzahl',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'Legt eine Obergrenze für erzeugte Frames fest, wenn die Frame-Erstellung im dynamischen Modus läuft.',
  'Up to 2x': 'Bis zu 2x',
  'Up to 3x': 'Bis zu 3x',
  'Up to 4x': 'Bis zu 4x',
  'Up to 5x': 'Bis zu 5x',
  'Up to 6x': 'Bis zu 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'Multi Frame Generation — dynamische Zielbildrate',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'Legt die Zielbildrate fest, die die dynamische Frame-Erstellung aufrechterhalten soll.',
  'Max Refresh Rate': 'Maximale Bildwiederholrate',
  'Forces a specific DLSS Ray Reconstruction preset.':
    'Erzwingt ein bestimmtes Preset für die DLSS Strahlenrekonstruktion.',
  'Preset D (Transformer Gen 1)': 'Voreinstellung D (Transformer Gen 1)',
  'Preset E (Transformer Gen 1)': 'Voreinstellung E (Transformer Gen 1)',
  'Preset F (Transformer Gen 2)': 'Voreinstellung F (Transformer Gen 2)',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'Überschreibt die im Spiel ausgewählte Qualitätsstufe für die DLSS Strahlenrekonstruktion.',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'Erzwingt im Spiel die neueste systemweit installierte DLSS-RR-Version. Wird von den meisten Titeln mit Strahlenrekonstruktion unterstützt.',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'de', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
