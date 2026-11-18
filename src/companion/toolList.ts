import type { FileInfo, Kind } from "../shared/types";

export interface ToolItem {
  id: string;
  path: string;
  name: string;
  size: number;
  pages: number | null;
}

/** Add files of `kind` after `existing`, in the order given, without duplicates. */
export function accept(files: FileInfo[], kind: Kind, existing: ToolItem[], makeId: () => string) {
  const items = [...existing];
  const rejected: string[] = [];
  for (const file of files) {
    if (file.kind !== kind) rejected.push(file.name);
    else if (!items.some((i) => i.path === file.path))
      items.push({ id: makeId(), path: file.path, name: file.name, size: file.size, pages: file.pages });
  }
  return { items, rejected };
}

const NOUN: Partial<Record<Kind, [string, string]>> = { pdf: ["a PDF", "PDFs"], image: ["an image", "images"] };

export function rejection(names: string[], kind: Kind): string | null {
  if (names.length === 0) return null;
  const [one, many] = NOUN[kind] ?? ["the right kind", "the right kind"];
  return names.length === 1 ? `${names[0]} isn't ${one}.` : `${names.length} files aren't ${many}.`;
}
