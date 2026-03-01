import $ from "@david/dax";

const TARGET = "aarch64-unknown-linux-musl";

console.log(`Building sape for ${TARGET} (Termux/Android)...`);
await $`cross build --release --target ${TARGET}`;

await Deno.mkdir("dist", { recursive: true });
await Deno.copyFile(`target/${TARGET}/release/sape`, "dist/sape-linux-arm64");
await $`tar -C dist -czf dist/sape-linux-arm64.tar.gz sape-linux-arm64`;

const stat = await Deno.stat("dist/sape-linux-arm64");
const mb = (stat.size / 1024 / 1024).toFixed(1);
console.log(`dist/sape-linux-arm64 (${mb}MB) — statically linked, runs on Termux`);
console.log("dist/sape-linux-arm64.tar.gz");
