// Downloads 7-Zip 23.01 x64 and extracts 7z.exe, 7z.dll and License.txt into src-tauri/bin/.
// 7zr.exe (the 7z-only standalone) opens the installer, which is a 7z SFX.
// Recast needs the full 7z.exe + 7z.dll: 7za.exe cannot unpack NSIS installers (Ghostscript).
// Both downloads are pinned by SHA-256 and checked before anything is executed.
// To move to a new 7-Zip release, update VERSION and both hashes together.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "2301";
const RELEASE = "23.01";
const PINNED = {
  "7zr.exe": "72c98287b2e8f85ea7bb87834b6ce1ce7ce7f41a8c97a81b307d4d4bf900922b",
  [`7z${VERSION}-x64.exe`]: "26cb6e9f56333682122fafe79dbcdfd51e9f47cc7217dccd29ac6fc33b5598cd",
};

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = join(root, ".cache", "7zip");
const out = join(root, "src-tauri", "bin");

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  writeFileSync(dest, Buffer.from(await res.arrayBuffer()));
}

/** Reuse a cached copy if it matches the pin; otherwise download and verify. */
async function fetchPinned(name) {
  const dest = join(work, name);
  if (existsSync(dest) && sha256(dest) === PINNED[name]) return dest;
  // The versioned release asset: 7-zip.org/a/7zr.exe always serves the newest 7zr.
  await download(`https://github.com/ip7z/7zip/releases/download/${RELEASE}/${name}`, dest);
  const got = sha256(dest);
  if (got !== PINNED[name]) {
    rmSync(dest, { force: true });
    throw new Error(`${name}: SHA-256 ${got} does not match the pinned ${PINNED[name]}`);
  }
  return dest;
}

mkdirSync(work, { recursive: true });
mkdirSync(out, { recursive: true });

const sevenzr = await fetchPinned("7zr.exe");
const installer = await fetchPinned(`7z${VERSION}-x64.exe`);

const x = join(work, "x");
rmSync(x, { recursive: true, force: true });
execFileSync(sevenzr, ["x", "-y", `-o${x}`, installer, "7z.exe", "7z.dll", "License.txt"], { stdio: "inherit" });
copyFileSync(join(x, "7z.exe"), join(out, "7z.exe"));
copyFileSync(join(x, "7z.dll"), join(out, "7z.dll"));
copyFileSync(join(x, "License.txt"), join(out, "7zip-License.txt"));

const formats = execFileSync(join(out, "7z.exe"), ["i"], { encoding: "utf8" });
if (!/\bNsis\b/i.test(formats)) throw new Error("7z.exe does not list the Nsis format");
for (const f of ["7z.exe", "7z.dll"]) {
  console.log(`${f}  sha256 ${sha256(join(out, f))}`);
}
console.log(`7-Zip ${VERSION} → src-tauri/bin`);
