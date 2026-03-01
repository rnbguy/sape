#!/usr/bin/env -S deno run -A
import { renderMermaidAscii } from "beautiful-mermaid";
import { basename, join } from "@std/path";

const DIAGRAMS_DIR = new URL("../docs/diagrams", import.meta.url).pathname;

const mmdFiles: string[] = [];
for await (const entry of Deno.readDir(DIAGRAMS_DIR)) {
  if (entry.isFile && entry.name.endsWith(".mmd")) {
    mmdFiles.push(entry.name);
  }
}
mmdFiles.sort();

if (mmdFiles.length === 0) {
  console.error("No .mmd files found in", DIAGRAMS_DIR);
  Deno.exit(1);
}

for (const mmd of mmdFiles) {
  const inPath = join(DIAGRAMS_DIR, mmd);
  const outPath = join(DIAGRAMS_DIR, mmd.replace(/\.mmd$/, ".txt"));
  const src = await Deno.readTextFile(inPath);
  const ascii = renderMermaidAscii(src, { useAscii: false });
  await Deno.writeTextFile(outPath, ascii + "\n");
  console.log(`${basename(inPath)} → ${basename(outPath)}`);
}
