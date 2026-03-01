#!/usr/bin/env -S deno run -A
/**
 * E2E test infrastructure setup/teardown.
 *
 * Usage:
 *   deno run -A tests/e2e_setup.ts build    # build Docker image only
 *   deno run -A tests/e2e_setup.ts setup    # build + network + disable IGMP snooping
 *   deno run -A tests/e2e_setup.ts teardown # remove containers + network
 *
 * IGMP snooping is disabled via a privileged container (no host sudo needed).
 * This allows mDNS multicast (224.0.0.251) to flow between containers.
 */
import $ from "@david/dax";
import { BRIDGE_NAME, IMAGE, NETWORK } from "./constants.ts";

async function buildImage(): Promise<void> {
  console.log(`Building Docker image: ${IMAGE}`);
  await $`docker build -t ${IMAGE} .`;
  console.log(`Image ${IMAGE} built successfully`);
}

async function createNetwork(): Promise<void> {
  await $`docker network rm ${NETWORK}`.noThrow().quiet();
  console.log(`Creating Docker network: ${NETWORK} (bridge: ${BRIDGE_NAME})`);
  await $`docker network create --driver bridge -o com.docker.network.bridge.name=${BRIDGE_NAME} ${NETWORK}`;
}

async function disableIgmpSnooping(): Promise<void> {
  const sysPath =
    `/sys/devices/virtual/net/${BRIDGE_NAME}/bridge/multicast_snooping`;

  console.log(
    `Disabling IGMP snooping on ${BRIDGE_NAME} via privileged container`,
  );
  // --network host so the container sees the host's bridge in /sys
  await $`docker run --rm --privileged --network host alpine sh -c ${
    "echo 0 > " + sysPath
  }`;

  // Verify
  const value = (
    await $`docker run --rm --privileged --network host alpine cat ${sysPath}`
      .text()
  ).trim();
  if (value !== "0") {
    throw new Error(
      `Failed to disable IGMP snooping: got "${value}", expected "0"`,
    );
  }
  console.log("IGMP snooping disabled — mDNS multicast will work");
}

async function teardown(): Promise<void> {
  console.log("Tearing down e2e infrastructure");

  const ps = await $`docker ps -a --filter name=e2e- --format ${"{{.Names}}"}`
    .noThrow()
    .text();
  for (const name of ps.split("\n").filter(Boolean)) {
    await $`docker rm -f ${name}`.noThrow().quiet();
  }

  await $`docker network rm ${NETWORK}`.noThrow().quiet();
  console.log("Teardown complete");
}

// --- CLI ---
const command = Deno.args[0];

switch (command) {
  case "build":
    await buildImage();
    break;
  case "setup":
    await buildImage();
    await createNetwork();
    await disableIgmpSnooping();
    console.log("E2E infrastructure ready");
    break;
  case "teardown":
    await teardown();
    break;
  default:
    console.error("Usage: e2e_setup.ts <build|setup|teardown>");
    Deno.exit(1);
}
