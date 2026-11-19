/** What a drag entering the pad window should do (pure; PadApp acts on it). */
export type EnterAction = "ignore" | "track" | "rebegin" | "park";

interface EnterSession {
  generation: number;
  paths: string[];
  probe: boolean;
}

const samePaths = (a: string[], b: string[]) => a.length === b.length && a.every((p, i) => p === b[i]);

export function enterAction(
  session: EnterSession | null,
  openGen: number,
  paths: string[],
  beginInFlight = false,
): EnterAction {
  // A text or link drag carries no paths: let the probe time out (Rust hides it).
  if (session?.probe && paths.length === 0) return "ignore";
  // No session yet, or a newer open is still loading: that open owns this drag. While a begin
  // is in flight, park the enter and replay it once the session exists (Rust already confirmed
  // the probe, so ignoring it would leave the pad invisible).
  if (!session || session.generation !== openGen) return beginInFlight ? "park" : "ignore";
  // Dragged files differ from the session's: target the files being dragged (A2).
  if (paths.length > 0 && !samePaths(paths, session.paths)) return "rebegin";
  return "track";
}
