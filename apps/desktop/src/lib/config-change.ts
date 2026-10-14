import { latest } from "./latest";
import type { ConfigChange } from "./types";

export type ConfigTarget = Pick<
  ConfigChange,
  "run" | "config_revision" | "source_path"
> & { id: string };

/** Coalesce focus/visibility checks, and keep delayed IPC replies tied to the loaded config. */
export function configChecker(
  target: () => ConfigTarget | null,
  load: (id: string) => Promise<ConfigChange | null>,
  apply: (result: ConfigChange) => void,
  fail: (error: unknown) => void
) {
  const jobs = latest();
  let pending: { key: string; promise: Promise<void> } | null = null;

  function cancel() {
    jobs.cancel();
    pending = null;
  }

  function check(): Promise<void> {
    const snapshot = target();
    if (!snapshot) {
      cancel();
      return Promise.resolve();
    }
    const key = JSON.stringify(snapshot);
    if (pending?.key === key) {
      return pending.promise;
    }
    const promise = jobs
      .run(async (current) => {
        const valid = () => current() && JSON.stringify(target()) === key;
        try {
          const result = await load(snapshot.id);
          if (
            valid() &&
            result &&
            result.run === snapshot.run &&
            result.config_revision === snapshot.config_revision &&
            result.source_path === snapshot.source_path
          ) {
            apply(result);
          }
        } catch (error) {
          if (valid()) {
            fail(error);
          }
        }
      })
      .finally(() => {
        if (pending?.promise === promise) {
          pending = null;
        }
      });
    pending = { key, promise };
    return promise;
  }

  return { cancel, check };
}
