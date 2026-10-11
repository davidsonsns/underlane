/**
 * Runs only the latest of a series of async jobs, each after a delay (debounce + generation
 * guard). A newer run(), or cancel(), supersedes the pending job: if its timer hasn't fired it
 * never starts, if it's in flight `current()` turns false so it drops its result. Either way the
 * promise run() returned resolves, so a caller awaiting a superseded job is never left hanging.
 * If the current job throws, that promise rejects (a superseded job's error is dropped).
 */
export function latest() {
  let gen = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let settle: (() => void) | undefined;

  function cancel() {
    gen++;
    clearTimeout(timer);
    settle?.();
    settle = undefined;
  }

  function run(job: (current: () => boolean) => Promise<void>, delay = 0): Promise<void> {
    cancel();
    const g = gen;
    const current = () => g === gen;
    return new Promise<void>((resolve, reject) => {
      settle = resolve;
      timer = setTimeout(async () => {
        try {
          await job(current);
          resolve();
        } catch (e) {
          // The current job's failure reaches the caller; a superseded job's is dropped with it.
          if (current()) reject(e);
          else resolve();
        }
      }, delay);
    });
  }

  return { run, cancel };
}
