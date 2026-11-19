/** Settings saves run one after another, and every successful answer is applied in order (a later failure doesn't lose an earlier success). */
export type SaveResult = { ok: true } | { ok: false; error: string };

export function createSaveQueue<P, R>(
  run: (patch: P) => Promise<R>,
  onResult: (r: R) => void,
  onError?: (e: string) => void,
): (patch: P) => Promise<SaveResult> {
  let chain: Promise<unknown> = Promise.resolve();
  return (patch) => {
    const step = chain.then(() => run(patch)).then(
      (r): SaveResult => {
        onResult(r);
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
