// Pins the latest engine releases into src-tauri/engines/engines.toml (spec §6.3):
// URL, sha256 and size per engine, then runs the capability checks before pinning.
// Usage: node scripts/refresh-engines.mjs   (set GITHUB_TOKEN to avoid API rate limits)
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream, existsSync, mkdirSync, mkdtempSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const cache = join(root, ".cache", "engines");
const sevenZip = join(root, "src-tauri", "bin", "7z.exe");

const ENGINES = [
  { id: "ffmpeg", name: "FFmpeg", repo: "GyanD/codexffmpeg", asset: /^ffmpeg-[\d.]+-essentials_build\.zip$/,
    licence: "GPL-3.0-or-later", homepage: "https://ffmpeg.org", source_url: "https://github.com/GyanD/codexffmpeg",
    bins: { ffmpeg: "ffmpeg.exe", ffprobe: "ffprobe.exe" } },
  { id: "imagemagick", name: "ImageMagick", repo: "ImageMagick/ImageMagick", asset: /portable-Q16-HDRI-x64\.(zip|7z)$/,
    licence: "ImageMagick", homepage: "https://imagemagick.org", source_url: "https://github.com/ImageMagick/ImageMagick",
    bins: { magick: "magick.exe" }, capabilities: "magick_formats" },
  { id: "pandoc", name: "Pandoc", repo: "jgm/pandoc", asset: /windows-x86_64\.zip$/,
    licence: "GPL-2.0-or-later", homepage: "https://pandoc.org", source_url: "https://github.com/jgm/pandoc",
    bins: { pandoc: "pandoc.exe" } },
  { id: "qpdf", name: "qpdf", repo: "qpdf/qpdf", asset: /msvc64\.zip$/,
    licence: "Apache-2.0", homepage: "https://qpdf.sourceforge.io", source_url: "https://github.com/qpdf/qpdf",
    bins: { qpdf: "qpdf.exe" } },
  { id: "ghostscript", name: "Ghostscript", repo: "ArtifexSoftware/ghostpdl-downloads", asset: /^gs\d+w64\.exe$/, archive: "nsis",
    licence: "AGPL-3.0-or-later", homepage: "https://ghostscript.com", source_url: "https://github.com/ArtifexSoftware/ghostpdl",
    bins: { gswin64c: "gswin64c.exe" }, check: ["gswin64c", "-v"] },
];

const headers = { "User-Agent": "recast-refresh-engines", Accept: "application/vnd.github+json",
  ...(process.env.GITHUB_TOKEN ? { Authorization: `Bearer ${process.env.GITHUB_TOKEN}` } : {}) };

async function latest(repo) {
  const r = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, { headers });
  if (!r.ok) throw new Error(`${repo}: GitHub API HTTP ${r.status}`);
  return r.json();
}

async function download(url, dest) {
  if (existsSync(dest)) return;
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(dest + ".part"));
  renameSync(dest + ".part", dest);
}

async function sha256(file) {
  const h = createHash("sha256");
  await pipeline(createReadStream(file), h);
  return h.digest("hex");
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]);
}
const find = (dir, name) => walk(dir).find((p) => basename(p).toLowerCase() === name.toLowerCase());

mkdirSync(cache, { recursive: true });
const pinned = [];
for (const e of ENGINES) {
  const rel = await latest(e.repo);
  const asset = rel.assets.find((a) => e.asset.test(a.name));
  if (!asset) throw new Error(`${e.id}: no asset matching ${e.asset} in ${rel.tag_name}`);
  const file = join(cache, asset.name);
  console.log(`${e.id}: ${asset.name}`);
  await download(asset.browser_download_url, file);
  const x = join(cache, "x", e.id);
  rmSync(x, { recursive: true, force: true });
  execFileSync(sevenZip, ["x", "-y", `-o${x}`, file], { stdio: "ignore" });
  for (const exe of Object.values(e.bins)) if (!find(x, exe)) throw new Error(`${e.id}: ${exe} not found after extraction`);
  pinned.push({ ...e, version: rel.tag_name.replace(/^v/, ""), url: asset.browser_download_url,
    sha256: await sha256(file), size: statSync(file).size,
    archive: e.archive ?? (asset.name.endsWith(".7z") ? "7z" : "zip") });
}

// Capability checks (spec §6.3): fail before pinning a build that lacks what the presets need.
const magick = find(join(cache, "x", "imagemagick"), "magick.exe");
const formats = execFileSync(magick, ["-list", "format"], { encoding: "utf8" }).split(/\r?\n/);
for (const [fmt, need] of [["HEIC", "r"], ["AVIF", "rw"], ["JXL", "rw"], ["WEBP", "rw"]]) {
  const t = formats.map((l) => l.trim().split(/\s+/)).find((t) => t[0]?.replace("*", "").toUpperCase() === fmt);
  // Some builds print a Module column between Format and Mode, some don't.
  const mode = [t?.[1], t?.[2]].find((c) => c && /^[rw+-]{3}$/.test(c));
  if (!mode || (need.includes("r") && mode[0] !== "r") || (need.includes("w") && mode[1] !== "w"))
    throw new Error(`ImageMagick build lacks ${fmt} ${need}`);
}
const gs = find(join(cache, "x", "ghostscript"), "gswin64c.exe");
const probe = mkdtempSync(join(tmpdir(), "recast-gs-"));
execFileSync(magick, ["-size", "100x100", "xc:white", join(probe, "t.pdf")]);
execFileSync(gs, ["-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=png16m", "-sOutputFile=out.png", join(probe, "t.pdf")], { cwd: probe, stdio: "ignore" });
if (!existsSync(join(probe, "out.png"))) throw new Error("gswin64c failed to run from an unpacked folder");

const q = (s) => JSON.stringify(s);
let toml = "# Generated by scripts/refresh-engines.mjs. Do not edit by hand.\n";
for (const e of pinned) {
  toml += `\n[[engine]]\nid = ${q(e.id)}\nname = ${q(e.name)}\nversion = ${q(e.version)}\nplatform = "windows-x64"\n`;
  toml += `url = ${q(e.url)}\nsha256 = ${q(e.sha256)}\nsize = ${e.size}\narchive = ${q(e.archive)}\n`;
  toml += `licence = ${q(e.licence)}\nhomepage = ${q(e.homepage)}\nsource_url = ${q(e.source_url)}\n`;
  if (e.check) toml += `check = [${e.check.map(q).join(", ")}]\n`;
  if (e.capabilities) toml += `capabilities = ${q(e.capabilities)}\n`;
  toml += `\n[engine.bins]\n${Object.entries(e.bins).map(([k, v]) => `${k} = ${q(v)}`).join("\n")}\n`;
}
mkdirSync(join(root, "src-tauri", "engines"), { recursive: true });
writeFileSync(join(root, "src-tauri", "engines", "engines.toml"), toml);
console.log("Wrote src-tauri/engines/engines.toml");
