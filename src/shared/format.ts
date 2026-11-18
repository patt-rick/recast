export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n;
  let i = -1;
  do {
    v /= 1024;
    i++;
  } while (v >= 1024 && i < units.length - 1);
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function formatEta(s: number | null): string {
  if (s == null || !Number.isFinite(s)) return "";
  const r = Math.max(0, Math.round(s));
  if (r < 60) return `${r}s left`;
  const m = Math.floor(r / 60);
  if (m < 60) return `${m}m ${String(r % 60).padStart(2, "0")}s left`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m left`;
}

export function baseName(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}
