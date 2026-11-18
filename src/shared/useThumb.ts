import { useEffect, useState } from "react";
import { ipc } from "./ipc";

/** Object URL of a file's thumbnail, or null (show the type glyph). Revoked on change/unmount. */
export function useThumb(path: string | null): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    setUrl(null);
    if (!path) return;
    let live = true;
    let made: string | null = null;
    ipc.thumbnail(path).then(
      (buf) => {
        if (!live || buf.byteLength === 0) return;
        made = URL.createObjectURL(new Blob([buf]));
        setUrl(made);
      },
      () => {},
    );
    return () => {
      live = false;
      if (made) URL.revokeObjectURL(made);
    };
  }, [path]);
  return url;
}
