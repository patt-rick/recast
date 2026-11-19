/** What a drop on the pad should do (pure, so the decision is testable). */
import type { Hit } from "./hitTest";

export type DropPlan =
  /** Nothing to do. */
  | "ignore"
  /** Same files as the session: run the targeted segment, and stay open as a click-mode pad. */
  | "run-and-keep"
  /** Same files, but the drop landed outside the ring: the reducer closes the pad. */
  | "run-and-close"
  /** Different files: open a fresh click-mode pad for them. */
  | "reopen";

export function planDrop(
  session: { waiting: boolean; paths: string[] },
  dropped: string[],
  hit: Hit,
): DropPlan {
  const same =
    !session.waiting && dropped.length === session.paths.length && dropped.every((p, i) => p === session.paths[i]);
  if (same) return hit.kind === "none" ? "run-and-close" : "run-and-keep";
  return dropped.length > 0 ? "reopen" : "ignore";
}

/** A drop that arrived before the session existed (the pad is shown before inspect/capture finish). */
export interface PendingDrop {
  x: number;
  y: number;
  paths: string[];
}
