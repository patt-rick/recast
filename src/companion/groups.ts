/** Mixed drops split into one group per kind (spec §5.6). */
import type { FileInfo, Kind } from "../shared/types";

export interface Group {
  kind: Kind;
  label: string;
  paths: string[];
  names: string[];
}

const ORDER: Kind[] = ["video", "audio", "image", "pdf", "text", "unknown"];
const NOUN: Record<Kind, [string, string]> = {
  video: ["video", "videos"],
  audio: ["audio file", "audio files"],
  image: ["image", "images"],
  pdf: ["PDF", "PDFs"],
  text: ["document", "documents"],
  unknown: ["other file", "other files"],
};

export function groupByKind(files: FileInfo[]): Group[] {
  return ORDER.flatMap((kind) => {
    const fs = files.filter((x) => x.kind === kind);
    if (fs.length === 0) return [];
    const [one, many] = NOUN[kind];
    return [{ kind, label: `${fs.length} ${fs.length === 1 ? one : many}`, paths: fs.map((x) => x.path), names: fs.map((x) => x.name) }];
  });
}
