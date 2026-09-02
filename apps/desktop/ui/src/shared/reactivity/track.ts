/**
 * Synchronously reads and registers reactive dependencies for Svelte 5 `$effect` or `$derived`.
 *
 * In Svelte 5, reactive dependencies are tracked dynamically when their getters are read.
 * When an effect or derived value needs to react to changes in state that is otherwise unused
 * in its synchronous body (or enclosed in `untrack`), passing the values to `track(...)`
 * ensures Svelte registers them without triggering linter warnings.
 *
 * @param _dependencies - Reactive signals or values to register as dependencies.
 */
export function track(..._dependencies: readonly unknown[]): void {
  // Intentional no-op: evaluates arguments to register reactive dependencies in Svelte.
}
