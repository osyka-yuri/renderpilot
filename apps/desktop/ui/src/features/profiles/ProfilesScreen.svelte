<script lang="ts">
  import type { GameCard } from '@shared/api/types';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';

  type Props = {
    gameCard?: GameCard | null;
    advancedMode?: boolean;
  };

  let { gameCard = null, advancedMode = false }: Props = $props();

  const profiles = [
    { name: 'Quality', note: 'Prefer image quality and safe replacement paths.', state: 'Preview' },
    { name: 'Balanced', note: 'Trade image quality for broader compatibility.', state: 'Preview' },
    {
      name: 'Performance',
      note: 'Surface aggressive recommendations only when capability data allows it.',
      state: 'Preview',
    },
    {
      name: 'Low Latency',
      note: 'Reserved for NVAPI and frame-generation capable games.',
      state: 'Experimental',
    },
  ];

  function profileStateTone(value: string): 'muted' | 'warning' {
    return value === 'Experimental' ? 'warning' : 'muted';
  }
</script>

<section class="screen-shell">
  <section class="hero-panel">
    <div>
      <p class="eyebrow">Profiles</p>
      <h2>Recommendation and preset surface</h2>
      <p>
        Profiles stay visible because the PRD requires them, but they remain preview-only until the
        recommendation engine and capability model are fully wired.
      </p>
    </div>

    <div class="hero-side">
      <strong>{gameCard?.title ?? 'No focused game selected'}</strong>
      <p>
        {advancedMode
          ? 'Advanced mode exposes experimental profile lanes.'
          : 'Enable Advanced Mode to reveal experimental profile lanes.'}
      </p>
    </div>
  </section>

  <div class="profile-grid">
    {#each profiles as profile}
      <article class="profile-card">
        <div class="card-head">
          <div>
            <p class="eyebrow">Profile</p>
            <h3>{profile.name}</h3>
          </div>
          <Badge pill tone={profileStateTone(profile.state)}>{profile.state}</Badge>
        </div>
        <p>{profile.note}</p>
        <Button variant="secondary" size="sm" disabled>Preview Only</Button>
      </article>
    {/each}
  </div>
</section>

<style>
  .screen-shell {
    display: grid;
    gap: 1rem;
  }

  .hero-panel,
  .profile-card {
    border-radius: 1.6rem;
    border: 1px solid var(--border-subtle);
    background: var(--bg-panel);
  }

  .hero-panel {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(260px, 0.7fr);
    gap: 1rem;
    padding: 1.25rem;
  }

  .hero-side,
  .profile-card {
    padding: 1rem;
    border-radius: 1.2rem;
    background: var(--bg-soft);
  }

  .profile-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 0.9rem;
  }

  .card-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: start;
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

  @media (max-width: 900px) {
    .hero-panel {
      grid-template-columns: 1fr;
    }
  }
</style>
