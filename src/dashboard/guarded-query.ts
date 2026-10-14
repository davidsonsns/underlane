import { errorText } from "$lib/api";

/** A stale view must not show an earlier query's failure or clear its loading state. */
export async function guardedQuery(
  work: () => Promise<void>,
  current: () => boolean,
  onError: (cause: unknown) => void,
  onFinish: () => void
) {
  try {
    await work();
  } catch (cause) {
    if (current()) {
      onError(cause);
    }
  } finally {
    if (current()) {
      onFinish();
    }
  }
}

export function queryTracker(status: {
  failure: string | null;
  loading: boolean;
}) {
  let active: Promise<void> | null = null;
  function run(work: () => Promise<void>, current: () => boolean) {
    const promise = guardedQuery(
      work,
      current,
      (cause) => {
        status.failure = errorText(cause);
      },
      () => {
        status.loading = false;
      }
    );
    active = promise;
    promise.finally(() => {
      if (active === promise) {
        active = null;
      }
    });
    return promise;
  }
  return {
    get active() {
      return active;
    },
    cancel() {
      active = null;
    },
    run,
  };
}
