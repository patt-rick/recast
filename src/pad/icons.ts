/** Line icons (24×24, stroked 1.75) for preset `icon` names and pad glyphs. */
const circle = (cx: number, cy: number, r: number) => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
const rect = (x: number, y: number, w: number, h: number, r: number) =>
  `M${x + r} ${y}h${w - 2 * r}a${r} ${r} 0 0 1 ${r} ${r}v${h - 2 * r}a${r} ${r} 0 0 1 ${-r} ${r}h${-(w - 2 * r)}a${r} ${r} 0 0 1 ${-r} ${-r}v${-(h - 2 * r)}a${r} ${r} 0 0 1 ${r} ${-r}z`;

const ICONS: Record<string, string> = {
  music: "M9 18V5l12-2v13" + circle(6, 18, 3) + circle(18, 16, 3),
  film: rect(3, 3, 18, 18, 3) + "M7.5 3v18M16.5 3v18M3 12h18M3 7.5h4.5M3 16.5h4.5M16.5 7.5H21M16.5 16.5H21",
  sparkle: "M12 3.5l1.7 5.1 5.1 1.7-5.1 1.7L12 17.1l-1.7-5.1-5.1-1.7 5.1-1.7zM19 15.5l.6 1.9 1.9.6-1.9.6-.6 1.9-.6-1.9-1.9-.6 1.9-.6z",
  globe: circle(12, 12, 9) + "M3 12h18M12 3c2.5 2.6 3.7 5.6 3.7 9s-1.2 6.4-3.7 9c-2.5-2.6-3.7-5.6-3.7-9S9.5 5.6 12 3z",
  shrink: "M4 14h6v6M20 10h-6V4M14 10l6.5-6.5M3.5 20.5L10 14",
  box: "M21 8l-9-5-9 5v8l9 5 9-5V8zM3 8l9 5 9-5M12 13v8",
  wave: "M3 12h1M7 8v8M11 5v14M15 8v8M19 10v4",
  image: rect(3, 5, 18, 14, 2) + circle(8.5, 10, 1.5) + "M21 16l-5-5-8 8",
  grid: rect(3, 3, 7, 7, 1.5) + rect(14, 3, 7, 7, 1.5) + rect(3, 14, 7, 7, 1.5) + rect(14, 14, 7, 7, 1.5),
  doc: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h6",
  stack: "M12 3l9 5-9 5-9-5 9-5zM3 12l9 5 9-5M3 16l9 5 9-5",
  split: "M8 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3M16 3h3a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-3M12 2v20",
  book: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20V3H6.5A2.5 2.5 0 0 0 4 5.5v14zM6.5 17A2.5 2.5 0 0 0 4 19.5 2.5 2.5 0 0 0 6.5 22H20v-5",
  text: "M4 6h16M4 10h16M4 14h10M4 18h12",
  more: circle(5, 12, 1.2) + circle(12, 12, 1.2) + circle(19, 12, 1.2),
  back: "M15 5l-7 7 7 7",
  download: "M12 4v11M7 10l5 5 5-5M5 20h14",
  warning: "M12 3l10 18H2L12 3zM12 10v5M12 18h.01",
  check: "M5 12l5 5 9-10",
};

export const iconPath = (name: string): string => ICONS[name] ?? ICONS.doc;

/** The disc's type glyph when there is no thumbnail. */
export const kindIcon = (kind: string): string =>
  ({ video: "film", audio: "music", image: "image", pdf: "doc", text: "text" } as Record<string, string>)[kind] ?? "doc";
