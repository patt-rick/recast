/** Release builds only: a page reload drops the window's listeners and loses its state. */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
}

/** F5, Ctrl/Cmd+R and Ctrl/Cmd+Shift+R (shift doesn't change `key`'s letter, only its case). */
export function isReloadKey(e: KeyLike): boolean {
  if (e.key === "F5") return true;
  return (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r";
}

export function blockReload(): void {
  if (import.meta.env.DEV) return;
  window.addEventListener(
    "keydown",
    (e) => {
      if (isReloadKey(e)) e.preventDefault();
    },
    true,
  );
}
