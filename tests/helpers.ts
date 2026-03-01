import $ from "@david/dax";
import { IMAGE, NETWORK } from "./constants.ts";

/**
 * Run a detached Docker container on the e2e bridge network.
 */
export async function dockerRun(
  name: string,
  args: string[],
  opts: {
    env?: Record<string, string>;
    ports?: string[];
    image?: string;
    entrypoint?: string;
  } = {},
): Promise<string> {
  const cmd = ["docker", "run", "-d", "--name", name, "--network", NETWORK];

  for (const [k, v] of Object.entries(opts.env ?? { RUST_LOG: "info" })) {
    cmd.push("-e", `${k}=${v}`);
  }
  for (const p of opts.ports ?? []) {
    cmd.push("-p", p);
  }
  if (opts.entrypoint) cmd.push("--entrypoint", opts.entrypoint);

  cmd.push(opts.image ?? IMAGE, ...args);

  await $`${cmd}`.quiet();
  return name;
}

/**
 * Run an interactive Docker container (foreground, stdin piped).
 * Auto-removed on exit.
 */
export async function dockerRunInteractive(
  name: string,
  args: string[],
  input: string,
  opts: { env?: Record<string, string>; image?: string } = {},
): Promise<{ code: number; combined: string }> {
  const cmd = [
    "docker", "run", "-i", "--rm",
    "--name", name,
    "--network", NETWORK,
  ];

  for (const [k, v] of Object.entries(opts.env ?? { RUST_LOG: "info" })) {
    cmd.push("-e", `${k}=${v}`);
  }
  cmd.push(opts.image ?? IMAGE, ...args);

  const result = await $`${cmd}`
    .stdin(new TextEncoder().encode(input))
    .noThrow()
    .captureCombined();

  return { code: result.code, combined: result.combined };
}

/**
 * Poll docker logs until `needle` appears or timeout.
 */
export async function waitForLog(
  container: string,
  needle: string,
  timeoutSec = 30,
): Promise<void> {
  const deadline = Date.now() + timeoutSec * 1000;
  while (Date.now() < deadline) {
    const logs = await $`docker logs ${container} 2>&1`.noThrow().text();
    if (logs.includes(needle)) return;
    await $.sleep(1000);
  }
  const finalLogs = await $`docker logs ${container} 2>&1`.noThrow().text();
  throw new Error(
    `Timeout (${timeoutSec}s) waiting for "${needle}" in ${container} logs.\n` +
      `--- Last logs ---\n${finalLogs}`,
  );
}

/**
 * Get container logs.
 */
export async function dockerLogs(container: string): Promise<string> {
  return await $`docker logs ${container} 2>&1`.noThrow().text();
}

/**
 * Force-remove containers by name (ignores errors).
 */
export async function dockerRm(...names: string[]): Promise<void> {
  for (const name of names) {
    await $`docker rm -f ${name}`.noThrow().quiet();
  }
}

/**
 * Execute a command inside a running container and return the output.
 * Retries on failure up to `retries` times with `delayMs` between attempts.
 */
export async function dockerExec(
  container: string,
  cmd: string[],
  retries = 10,
  delayMs = 1000,
): Promise<string> {
  for (let i = 0; i < retries; i++) {
    const result = await $`docker exec ${container} ${cmd}`.noThrow().captureCombined();
    if (result.code === 0) return result.combined;
    if (i === retries - 1) {
      throw new Error(
        `docker exec in ${container} failed after ${retries} attempts: ${result.combined}`,
      );
    }
    await $.sleep(delayMs);
  }
  throw new Error("unreachable");
}
