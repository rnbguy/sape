import { assert } from "jsr:@std/assert";
import {
  DIALER_SEED,
  LISTENER_PEER_ID,
  LISTENER_SEED,
  RELAY_PEER_ID,
  RELAY_SEED,
} from "./constants.ts";
import {
  dockerExec,
  dockerLogs,
  dockerRm,
  dockerRun,
  waitForLog,
} from "./helpers.ts";

Deno.test("relay reverse-forward: TCP reverse forwarding over relay circuit", async () => {
  const containers = [
    "e2e-relay-rfwd-relay",
    "e2e-relay-rfwd-http",
    "e2e-relay-rfwd-listener",
    "e2e-relay-rfwd-dialer",
  ];
  try {
    // 1. Start relay
    await dockerRun("e2e-relay-rfwd-relay", [
      "relay",
      "--secret-key-seed",
      RELAY_SEED,
      "--port",
      "4001",
    ]);
    await waitForLog("e2e-relay-rfwd-relay", "relay listening address");

    // 2. Start HTTP server (will be the target on the dialer's network)
    await dockerRun(
      "e2e-relay-rfwd-http",
      [
        "run",
        "-A",
        "jsr:@std/http/file-server",
        "-p",
        "9000",
        "--host",
        "0.0.0.0",
      ],
      {
        image: "denoland/deno",
      },
    );

    // 3. Start listener with relay
    const relayAddr =
      `/dns4/e2e-relay-rfwd-relay/tcp/4001/p2p/${RELAY_PEER_ID}`;
    await dockerRun("e2e-relay-rfwd-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
      "--relay-address",
      relayAddr,
    ]);
    await waitForLog("e2e-relay-rfwd-listener", "Relay dial address:");

    // 4. Start dialer with reverse-forward
    //    Binds 9090 on LISTENER side, forwards to e2e-relay-rfwd-http:9000 (dialer side)
    const circuitAddr = `${relayAddr}/p2p-circuit/p2p/${LISTENER_PEER_ID}`;
    await dockerRun(
      "e2e-relay-rfwd-dialer",
      [
        "dial",
        "-R",
        "9090:e2e-relay-rfwd-http:9000",
        circuitAddr,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog(
      "e2e-relay-rfwd-dialer",
      "reverse forward request accepted",
    );

    // 5. Curl from inside the LISTENER container to the reverse-forwarded port
    const body = await dockerExec(
      "e2e-relay-rfwd-listener",
      ["curl", "-sf", "http://127.0.0.1:9090/"],
    );

    assert(body.length > 0, `Expected non-empty HTTP response body`);
  } catch (err) {
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
