/** One drag-drop listener for the companion, routed to the section that should get it. */
export type DragEv = { type: "enter" } | { type: "over" } | { type: "leave" } | { type: "drop"; paths: string[] };
export type DragTarget = "activity" | "tools";
type Handler = (e: DragEv) => void;

export function createRouter() {
  const handlers = new Map<DragTarget, Handler>();
  return {
    subscribe(t: DragTarget, h: Handler) {
      handlers.set(t, h);
      return () => {
        if (handlers.get(t) === h) handlers.delete(t);
      };
    },
    route(t: DragTarget, e: DragEv) {
      handlers.get(t)?.(e);
    },
    targetFor(section: string): DragTarget {
      return section === "tools" ? "tools" : "activity";
    },
  };
}

export const router = createRouter();
