// Downloads the current 7-Zip x64 release and extracts 7z.exe, 7z.dll and License.txt
// into src-tauri/bin/. 7zr.exe (the 7z-only standalone) opens the installer, which is a 7z SFX.
// Recast needs the full 7z.exe + 7z.dll: 7za.exe cannot unpack NSIS installers (Ghostscript).
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const work = join(root, ".cache", "7zip");
const out = join(root, "src-tauri", "bin");

async function download(url, dest) {
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  writeFileSync(dest, Buffer.from(await res.arrayBuffer()));
}

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
mkdirSync(out, { recursive: true });

const page = await (await fetch("https://www.7-zip.org/download.html")).text();
const m = page.match(/a\/7z(\d{4})-x64\.exe/);
if (!m) throw new Error("No x64 installer link on 7-zip.org/download.html");
const installer = join(work, `7z${m[1]}-x64.exe`);
await download(`https://www.7-zip.org/${m[0]}`, installer);
await download("https://www.7-zip.org/a/7zr.exe", join(work, "7zr.exe"));

execFileSync(join(work, "7zr.exe"), ["x", "-y", `-o${join(work, "x")}`, installer, "7z.exe", "7z.dll", "License.txt"], { stdio: "inherit" });
copyFileSync(join(work, "x", "7z.exe"), join(out, "7z.exe"));
copyFileSync(join(work, "x", "7z.dll"), join(out, "7z.dll"));
copyFileSync(join(work, "x", "License.txt"), join(out, "7zip-License.txt"));

const formats = execFileSync(join(out, "7z.exe"), ["i"], { encoding: "utf8" });
if (!/\bNsis\b/i.test(formats)) throw new Error("7z.exe does not list the Nsis format");
for (const f of ["7z.exe", "7z.dll"]) {
  console.log(`${f}  sha256 ${createHash("sha256").update(readFileSync(join(out, f))).digest("hex")}`);
}
console.log(`7-Zip ${m[1]} → src-tauri/bin`);
