/** Captured backdrop → pixels for luminance + pre-processed glass bitmaps (spec §4.4, §6.8). */
import { SIZE } from "../geometry";
import type { Pixels } from "../luminance";

export interface GlassSource {
  /** Unblurred (the start of the open animation). */
  raw: ImageBitmap;
  /** Over dark backdrops: blur 5, saturate 1.6, brightness 1.08. */
  dark: ImageBitmap;
  /** Over light backdrops: blur 6, saturate 1.2, brightness 1.0. */
  light: ImageBitmap;
  pixels: Pixels;
  /** Physical px per logical px. */
  scale: number;
}

export function decode(buf: ArrayBuffer): Pixels {
  if (buf.byteLength < 8) throw new Error("backdrop: no header");
  const v = new DataView(buf);
  const width = v.getUint32(0, true), height = v.getUint32(4, true);
  if (buf.byteLength !== 8 + width * height * 4) throw new Error("backdrop: wrong length");
  return { data: new Uint8ClampedArray(buf, 8), width, height };
}

async function filtered(src: ImageBitmap, filter: string): Promise<ImageBitmap> {
  const c = new OffscreenCanvas(src.width, src.height);
  const x = c.getContext("2d")!;
  x.filter = filter;
  x.drawImage(src, 0, 0);
  return c.transferToImageBitmap();
}

/** Done once per open; the per-frame work only clips and transforms these. */
export async function prepare(buf: ArrayBuffer): Promise<GlassSource> {
  const pixels = decode(buf);
  const scale = pixels.width / SIZE;
  const raw = await createImageBitmap(new ImageData(pixels.data, pixels.width, pixels.height));
  const [dark, light] = await Promise.all([
    filtered(raw, `blur(${5 * scale}px) saturate(1.6) brightness(1.08)`),
    filtered(raw, `blur(${6 * scale}px) saturate(1.2) brightness(1)`),
  ]);
  return { raw, dark, light, pixels, scale };
}

/** Drop the snapshot when the pad closes (§8: it never outlives the pad). */
export function release(g: GlassSource | null) {
  g?.raw.close();
  g?.dark.close();
  g?.light.close();
}
