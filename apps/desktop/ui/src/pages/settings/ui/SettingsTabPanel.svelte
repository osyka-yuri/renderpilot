<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ScrollArea, TabsContent } from '@shared/ui';

  type Props = {
    value: string;
    children: Snippet;
  };

  const { value, children }: Props = $props();
</script>

<TabsContent {value} class="min-h-0 flex-1 overflow-hidden">
  <ScrollArea class="h-full">
    <div class="flex flex-col gap-6">
      {@render children()}
    </div>
  </ScrollArea>
</TabsContent>
