import { describe, expect, it } from "vitest";
import { createSaveQueue } from "./saveQueue";

const defer = <T>() => {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((a, b) => ((resolve = a), (reject = b)));
  return { promise, resolve, reject };
};
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("createSaveQueue", () => {
  it("runs saves one at a time, in order", async () => {
    const started: number[] = [];
    const gates = [defer<number>(), defer<number>()];
    const save = createSaveQueue((n: number) => (started.push(n), gates[n - 1].promise), () => {});
    void save(1);
    void save(2);
    await tick();
    expect(started).toEqual([1]);
    gates[0].resolve(1);
    await tick();
    expect(started).toEqual([1, 2]);
  });

  it("applies every successful answer, in order", async () => {
    const applied: number[] = [];
    const save = createSaveQueue(async (n: number) => n, (r) => applied.push(r));
    await Promise.all([save(1), save(2), save(3)]);
    expect(applied).toEqual([1, 2, 3]);
  });

  it("applies an older answer when nothing newer was asked", async () => {
    const applied: number[] = [];
    const save = createSaveQueue(async (n: number) => n, (r) => applied.push(r));
    await save(1);
    await save(2);
    expect(applied).toEqual([1, 2]);
  });

  it("reports a failure, keeps the chain alive and resolves not-ok", async () => {
    const errors: string[] = [];
    const applied: number[] = [];
    const save = createSaveQueue(
      async (n: number) => {
        if (n === 1) throw new Error("disk full");
        return n;
      },
      (r) => applied.push(r),
      (e) => errors.push(e),
    );
    const [a, b] = await Promise.all([save(1), save(2)]);
    expect(a).toEqual({ ok: false, error: "Error: disk full" });
    expect(b).toEqual({ ok: true });
    expect(errors).toEqual(["Error: disk full"]);
    expect(applied).toEqual([2]);
  });

  it("keeps earlier successes when a later save fails", async () => {
    const errors: string[] = [];
    const applied: number[] = [];
    const save = createSaveQueue(
      async (n: number) => {
        if (n === 3) throw new Error("boom");
        return n;
      },
      (r) => applied.push(r),
      (e) => errors.push(e),
    );
    const results = await Promise.all([save(1), save(2), save(3)]);
    expect(applied).toEqual([1, 2]);
    expect(results[2]).toEqual({ ok: false, error: "Error: boom" });
    expect(errors).toEqual(["Error: boom"]);
  });
});
