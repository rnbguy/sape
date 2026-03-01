import { assert } from "jsr:@std/assert";
import {
  DIALER_SEED,
  LISTENER_PEER_ID,
  LISTENER_SEED,
  RELAY_PEER_ID,
  RELAY_SEED,
} from "./constants.ts";
import { dockerExec, dockerLogs, dockerRm, dockerRun, waitForLog } from "./helpers.ts";

Deno.test("relay local-forward: TCP forwarding over relay circuit", async () => {
  const containers = [
    "e2e-relay-fwd-relay",
    "e2e-relay-fwd-http",
    "e2e-relay-fwd-listener",
    "e2e-relay-fwd-dialer",
  ];
  try {
    // 1. Start relay server
    await dockerRun("e2e-relay-fwd-relay", [
      "relay",
      "--secret-key-seed",
      RELAY_SEED,
      "--port",
      "4001",
    ]);
    await waitForLog("e2e-relay-fwd-relay", "relay listening address");

    // 2. Start HTTP server as forwarding target
    await dockerRun(
      "e2e-relay-fwd-http",
      [
        "run", "-A",
        "jsr:@std/http/file-server",
        "-p", "9000",
        "--host", "0.0.0.0",
      ],
      {
        image: "denoland/deno",
      },
    );

    // 3. Start listener with relay
    const relayAddr =
      `/dns4/e2e-relay-fwd-relay/tcp/4001/p2p/${RELAY_PEER_ID}`;
    await dockerRun("e2e-relay-fwd-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
      "--relay-address",
      relayAddr,
    ]);
    await waitForLog("e2e-relay-fwd-listener", "Relay dial address:");

    // 4. Start dialer with local-forward
    const circuitAddr =
      `${relayAddr}/p2p-circuit/p2p/${LISTENER_PEER_ID}`;
    await dockerRun(
      "e2e-relay-fwd-dialer",
      [
        "dial",
        "-L",
        "8080:e2e-relay-fwd-http:9000",
        circuitAddr,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog("e2e-relay-fwd-dialer", "local forward is listening");

    // 5. Curl from inside the dialer container (bound to 127.0.0.1:8080)
    const body = await dockerExec(
      "e2e-relay-fwd-dialer",
      ["curl", "-sf", "http://127.0.0.1:8080/"],
    );

    assert(body.length > 0, `Expected non-empty HTTP response body`);
  } catch (err) {
    // Capture logs before cleanup for debugging
    console.error("--- Test failed, capturing container logs ---");
    for (const c of containers) {
      try {
        const logs = await dockerLogs(c);
        console.error(`--- ${c} ---\n${logs}`);
      } catch { /* container may not exist */ }
    }
    throw err;
  } finally {
    await dockerRm(...containers);
  }
});
