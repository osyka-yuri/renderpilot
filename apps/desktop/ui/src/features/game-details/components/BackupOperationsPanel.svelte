<script lang="ts">
  import type { OperationSummary } from '@shared/api/types';
  import { formatLabel, formatTimestamp, statusTone } from '@shared/utils/presenters';
  import Badge from '@shared/ui/Badge.svelte';

  export let operations: OperationSummary[] = [];
</script>

<section class="content-section">
  <div class="section-head">
    <div>
      <p class="eyebrow">Backups</p>
      <h3>Backups</h3>
    </div>
    <Badge surface="outline" tone="muted">{operations.length} backup-ready operations</Badge>
  </div>

  {#if operations.length === 0}
    <div class="empty-inline">No backup snapshots exist yet for this game installation.</div>
  {:else}
    <div class="backup-list">
      {#each operations as operation}
        <article class="backup-card">
          <div class="backup-head">
            <strong>{formatLabel(operation.kind)}</strong>
            <Badge pill tone={statusTone(operation.status)}>{formatLabel(operation.status)}</Badge>
          </div>
          <p>{formatTimestamp(operation.completed_at ?? operation.created_at)}</p>
          <div class="backup-metrics two-up">
            <div>
              <span>Backup status</span>
              <strong>{formatLabel(operation.backup_status)}</strong>
            </div>
            <div>
              <span>Snapshots</span>
              <strong>{operation.backup_count}</strong>
            </div>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>

<style>
  .content-section,
  .backup-list {
    display: grid;
    gap: var(--space-3);
  }

  .section-head,
  .backup-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .section-head {
    align-items: end;
    padding: 0 var(--space-1);
  }

  .eyebrow {
    margin: 0 0 0.2rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .backup-card,
  .empty-inline {
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-xl);
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--bg-card) 96%, white 4%),
      var(--bg-card)
    );
    box-shadow: var(--shadow-card);
  }

  .empty-inline {
    border-style: dashed;
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    box-shadow: none;
    color: var(--text-muted);
  }

  .backup-list {
    gap: 0.5rem;
  }

  .backup-card {
    display: grid;
    gap: var(--space-3);
  }

  .backup-card p {
    margin: 0;
    color: var(--text-muted);
  }

  .backup-metrics {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border-subtle);
  }

  .two-up {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .backup-metrics > div {
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-soft);
  }

  .backup-metrics span {
    display: block;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .backup-metrics strong,
  .backup-head strong {
    color: var(--text-strong);
  }

  .backup-metrics strong {
    display: block;
    overflow-wrap: anywhere;
    font-size: 0.92rem;
    line-height: 1.25;
  }

  @media (max-width: 820px) {
    .two-up {
      grid-template-columns: 1fr;
    }

    .section-head,
    .backup-head {
      flex-direction: column;
      align-items: flex-start;
    }

    .backup-card {
      padding: var(--space-3);
    }
  }
</style>
