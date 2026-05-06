<script lang="ts">
  import type { GameCard } from "@shared/api/types";
  import { formatRisk } from "@shared/utils/presenters";

  export let gameCard: GameCard | null = null;

  const previewRows = [
    {
      name: "nvngx_dlss.dll",
      technology: "DLSS Super Resolution",
      source: "Local observed",
      trust: "Local observed",
    },
    {
      name: "sl.common.dll",
      technology: "NVIDIA Streamline",
      source: "Backup import",
      trust: "Backup",
    },
    {
      name: "libxess.dll",
      technology: "Intel XeSS",
      source: "User import",
      trust: "User imported",
    },
  ];
</script>

<section class="screen-shell">
  <section class="hero-panel">
    <div>
      <p class="eyebrow">Library</p>
      <h2>Local artifact library</h2>
      <p>
        This surface is reserved for DLL and artifact inventory, with version,
        SHA256, source, and trust level. Until the dedicated endpoint exists,
        the UI shows a faithful placeholder layout instead of pretending data
        already ships.
      </p>
    </div>

    <div class="hero-side">
      <strong>Focused game</strong>
      <p>
        {gameCard
          ? `${gameCard.title} · Risk ${formatRisk(gameCard.risk_level)}`
          : "No focused game selected"}
      </p>
    </div>
  </section>

  <section class="preview-table">
    <header>
      <div>
        <p class="eyebrow">Artifact Inventory</p>
        <h3>Preview layout</h3>
      </div>
      <span>Awaiting real library endpoint</span>
    </header>

    <div class="row-grid header-row">
      <span>Artifact</span>
      <span>Technology</span>
      <span>Source</span>
      <span>Trust</span>
    </div>

    {#each previewRows as row}
      <div class="row-grid">
        <strong>{row.name}</strong>
        <span>{row.technology}</span>
        <span>{row.source}</span>
        <span>{row.trust}</span>
      </div>
    {/each}
  </section>
</section>

<style>
  .screen-shell {
    display: grid;
    gap: 1rem;
  }

  .hero-panel,
  .preview-table {
    border-radius: 1.6rem;
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel);
    padding: 1.25rem;
  }

  .hero-panel {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(260px, 0.7fr);
    gap: 1rem;
  }

  .hero-side {
    padding: 1rem;
    border-radius: 1.2rem;
    background: var(--bg-soft);
  }

  .eyebrow {
    margin: 0 0 0.35rem;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 0.72rem;
  }

  h2,
  h3 {
    margin: 0;
  }

  .preview-table header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: end;
    margin-bottom: 1rem;
  }

  .row-grid {
    display: grid;
    grid-template-columns: 1.2fr 1fr 1fr 0.8fr;
    gap: 0.8rem;
    padding: 0.9rem 1rem;
    border-radius: 1rem;
    background: var(--bg-soft);
  }

  .row-grid + .row-grid {
    margin-top: 0.6rem;
  }

  .header-row {
    margin-bottom: 0.6rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.78rem;
  }

  @media (max-width: 720px) {
    .hero-panel,
    .row-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
