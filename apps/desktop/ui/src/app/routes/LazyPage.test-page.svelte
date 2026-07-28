<script lang="ts">
  type Props = {
    label: string;
  };

  const { label }: Props = $props();
</script>

<p data-testid="loaded-page">{label}</p>
