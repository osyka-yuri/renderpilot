<script lang="ts">
  import type { LibrarySection } from '@features/graphics-configurator';
  import LibrarySectionCard from './LibrarySectionCard.svelte';

  type Props = {
    section: LibrarySection;
    selectedArtifacts?: Record<string, string>;
    selectedNvapiSelections?: Record<string, string>;
    riskLevel?: string | null | undefined;
    busy?: boolean;
    selectionKey: (componentId: string, controlId: string) => string;
    onArtifactSelection: (componentId: string, value: string) => void;
    onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
    onBuildPlan: (componentId: string, artifactId: string) => void;
  };

  const {
    section,
    selectedArtifacts = {},
    selectedNvapiSelections = {},
    riskLevel = null,
    busy = false,
    selectionKey,
    onArtifactSelection,
    onNvapiSelection,
    onBuildPlan,
  }: Props = $props();
</script>

<LibrarySectionCard
  eyebrow="Technology"
  {section}
  {selectedArtifacts}
  {selectedNvapiSelections}
  {riskLevel}
  {busy}
  {selectionKey}
  {onArtifactSelection}
  {onNvapiSelection}
  {onBuildPlan}
/>
