<script lang="ts">
  import type { GameDetails, GameGraphicsComponent } from '@entities/game';
  import {
    libraryVendorOrder,
    libraryVendorKey,
    vendorLabelForLibraryVendorKey,
    type LibraryVendorKey,
  } from '@shared/graphics';
  import { fileNameFromPath } from '@shared/path';
  import DownloadIcon from '@lucide/svelte/icons/download';
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import {
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
    Card,
    CardContent,
    CardDescription,
    CardTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    ScrollArea,
    Button,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import type { SwapHandler, RollbackHandler } from '../model/create-game-details-page-model';

  type Props = {
    details?: GameDetails | null;
    busy?: boolean;
    onSwap?: SwapHandler;
    onRollback?: RollbackHandler;
  };

  const {
    details = null,
    busy = false,
    onSwap = () => undefined,
    onRollback = () => undefined,
  }: Props = $props();

  type VendorTab = {
    key: LibraryVendorKey;
    label: string;
    components: GameGraphicsComponent[];
  };

  const tabs = $derived.by((): VendorTab[] => {
    if (!details) return [];

    const byVendor: Record<LibraryVendorKey, GameGraphicsComponent[]> = {
      nvidia: [],
      amd: [],
      intel: [],
      other: [],
    };

    for (const component of details.components) {
      const key = libraryVendorKey(component.technology);
      byVendor[key].push(component);
    }

    return libraryVendorOrder
      .map((key) => ({
        key,
        label: vendorLabelForLibraryVendorKey(key),
        components: byVendor[key],
      }))
      .filter((tab) => tab.components.length > 0);
  });

  function getCandidateGroup(componentId: string) {
    return details?.candidate_groups.find((g) => g.component_id === componentId) ?? null;
  }

  function handleSelection(componentId: string, value: string) {
    if (busy || !value) return;
    const group = getCandidateGroup(componentId);
    const candidate = group?.candidates.find((c) => c.artifact_id === value);
    void onSwap(componentId, value, candidate?.manifest_entry_id ?? null);
  }

  function handleRollbackClick(componentId: string) {
    if (busy) return;
    void onRollback(componentId);
  }
</script>

<ScrollArea class="h-full min-h-0">
  <section class="grid gap-4 p-1">
    {#if !details}
      <Card>
        <CardContent>
          <CardTitle>No game selected</CardTitle>
          <CardDescription>
            Select a game card on the dashboard to open one coherent workspace for that
            installation.
          </CardDescription>
        </CardContent>
      </Card>
    {:else if tabs.length === 0}
      <Card>
        <CardContent>
          <CardTitle>No graphics components detected</CardTitle>
          <CardDescription>
            No graphics-related components were detected for this installation.
          </CardDescription>
        </CardContent>
      </Card>
    {:else}
      <Tabs value={tabs[0].key}>
        <TabsList>
          {#each tabs as tab (tab.key)}
            <TabsTrigger value={tab.key}>{tab.label}</TabsTrigger>
          {/each}
        </TabsList>

        {#each tabs as tab (tab.key)}
          <TabsContent value={tab.key} class="grid gap-3">
            {#each tab.components as component (component.id)}
              {@const group = getCandidateGroup(component.id)}
              {@const filePath = component.files[0]?.path ?? 'Unknown'}
              {@const fileName = fileNameFromPath(filePath)}
              {@const candidates = group?.candidates ?? []}

              <Card>
                <CardContent class="grid gap-3">
                  <div class="grid gap-1">
                    <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                      {fileName}
                    </p>
                    <p class="text-sm break-all text-muted-foreground">{filePath}</p>
                  </div>

                  <div class="grid gap-1">
                    <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
                      Version
                    </p>

                    {#if candidates.length === 0}
                      <p class="text-sm text-muted-foreground">No replacement versions available</p>
                    {:else}
                      <div class="flex items-center gap-2">
                        <div class="min-w-0 flex-1">
                          <Select
                            type="single"
                            disabled={busy}
                            onValueChange={(value: string) => {
                              handleSelection(component.id, value);
                            }}
                          >
                            <SelectTrigger size="sm" class="w-full">
                              {group?.current_version ?? 'Unknown'}
                            </SelectTrigger>
                            <SelectContent>
                              {#each candidates as candidate (candidate.artifact_id)}
                                {@const versionLabel = `v${candidate.version ?? 'Unknown'}`}
                                <SelectItem value={candidate.artifact_id} label={versionLabel}>
                                  <span class="flex items-center gap-2">
                                    {versionLabel}
                                    {#if !candidate.is_downloaded}
                                      <DownloadIcon
                                        class="size-4 text-muted-foreground"
                                        aria-hidden="true"
                                      />
                                    {/if}
                                  </span>
                                </SelectItem>
                              {/each}
                            </SelectContent>
                          </Select>
                        </div>
                        {#if component.rollback_available}
                          <Tooltip>
                            <TooltipTrigger>
                              <Button
                                variant="ghost"
                                size="icon"
                                disabled={busy}
                                onclick={() => {
                                  handleRollbackClick(component.id);
                                }}
                                aria-label="Restore original version"
                              >
                                <Undo2Icon class="size-4" aria-hidden="true" />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>Restore original version</TooltipContent>
                          </Tooltip>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </CardContent>
              </Card>
            {/each}
          </TabsContent>
        {/each}
      </Tabs>
    {/if}
  </section>
</ScrollArea>
