/** Per-slot backdrop luminance for light/dark adaptation (spec §4.4, deviation 4). */
import { CX, CY, DISC_R, GAP, R, R1, TH, inSector, slotAngle, step } from "./geometry";

export interface Pixels {
  data: Uint8ClampedArray<ArrayBuffer>;
  width: number;
  height: number;
}

export const LIGHT_THRESHOLD = 0.6;
/** Sample every 4 logical px. */
const STEP_PX = 4;

export const luma = (r: number, g: number, b: number) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
export const isLight = (l: number) => l > LIGHT_THRESHOLD;

function mean(px: Pixels, scale: number, inside: (x: number, y: number) => boolean, half: number): number {
  let sum = 0, n = 0;
  for (let y = CY - half; y <= CY + half; y += STEP_PX)
    for (let x = CX - half; x <= CX + half; x += STEP_PX) {
      if (!inside(x, y)) continue;
      const px_ = Math.min(px.width - 1, Math.max(0, Math.round(x * scale)));
      const py = Math.min(px.height - 1, Math.max(0, Math.round(y * scale)));
      const o = (py * px.width + px_) * 4;
      sum += luma(px.data[o], px.data[o + 1], px.data[o + 2]);
      n++;
    }
  return n ? sum / n : 0;
}

/** Mean luma (0..1) of the backdrop under slot i of n at rest. `scale`: physical px per logical px. */
export const slotLuminance = (px: Pixels, scale: number, i: number, n: number) =>
  mean(px, scale, (x, y) => inSector(x, y, CX, CY, R, TH, slotAngle(i, n), step(n) / 2, n === 1 ? 0 : GAP), R1);

/** Mean luma under the centre's icon and name (a box, not the whole circle: that is what the text sits on). */
export const discLuminance = (px: Pixels, scale: number) =>
  mean(px, scale, (x, y) => Math.abs(x - CX) <= 50 && y >= CY - 32 && y <= CY + 34, DISC_R);

/** Luminance of every slot of a page of n slots. */
export const allSlotLuminance = (px: Pixels, scale: number, n: number) =>
  Array.from({ length: n }, (_, i) => slotLuminance(px, scale, i, n));
