/** When the first-run welcome shows, and when the edge handle may pulse for it. */
export function showWelcome(o: { loaded: boolean; firstRunDone: boolean; closed: boolean; readOnly: boolean | null }): boolean {
  // readOnly null = not known yet; a welcome that can't save its "done" would return every launch.
  return o.loaded && !o.firstRunDone && !o.closed && o.readOnly === false;
}

/** Nobody can see the handle's label unless the companion is on screen and in front. */
export function shouldHighlight(o: { step: number; visible: boolean; focused: boolean }): boolean {
  return o.step === 0 && o.visible && o.focused;
}
