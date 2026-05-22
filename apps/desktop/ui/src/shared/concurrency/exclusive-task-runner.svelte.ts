export type ExclusiveTaskRunnerOptions = {
  onBeforeRun?: () => void;
  onError?: (error: unknown) => void;
};

export type ExclusiveTaskRunner = {
  readonly busy: boolean;
  run<T>(task: () => Promise<T>, runOptions?: ExclusiveTaskRunnerOptions): Promise<T | null>;
};

export function createExclusiveTaskRunner(
  defaultOptions: ExclusiveTaskRunnerOptions = {},
): ExclusiveTaskRunner {
  let busy = $state(false);

  async function run<T>(
    task: () => Promise<T>,
    runOptions: ExclusiveTaskRunnerOptions = {},
  ): Promise<T | null> {
    if (busy) {
      return null;
    }

    busy = true;

    const onBeforeRun = runOptions.onBeforeRun ?? defaultOptions.onBeforeRun;
    const onError = runOptions.onError ?? defaultOptions.onError;

    onBeforeRun?.();

    try {
      return await task();
    } catch (error) {
      onError?.(error);
      return null;
    } finally {
      busy = false;
    }
  }

  return {
    get busy() {
      return busy;
    },
    run,
  };
}
