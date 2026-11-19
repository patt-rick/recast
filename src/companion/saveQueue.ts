/** Settings saves run one after another, and only the newest request's answer is applied. */
export type SaveResult = { ok: true } | { ok: false; error: string };

export function createSaveQueue<P, R>(
  run: (patch: P) => Promise<R>,
  onResult: (r: R) => void,
  onError?: (e: string) => void,
): (patch: P) => Promise<SaveResult> {
  let chain: Promise<unknown> = Promise.resolve();
  let latest = 0;
  return (patch) => {
    const seq = ++latest;
    const step = chain.then(() => run(patch)).then(
      (r): SaveResult => {
        // A newer save is queued behind this one: its answer carries both changes.
        if (seq === latest) onResult(r);
        return { ok: true };
      },
      (e): SaveResult => {
        const error = String(e);
        onError?.(error);
        return { ok: false, error };
      },
    );
    chain = step;
    return step;
  };
}
