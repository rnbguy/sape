import { assert } from "@std/assert";
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

Deno.test("relay http-connect: HTTP proxy over relay circuit", async () => {
  const containers = [
    "e2e-relay-hc-relay",
    "e2e-relay-hc-http",
    "e2e-relay-hc-listener",
    "e2e-relay-hc-dialer",
  ];
  try {
    // 1. Start relay
    await dockerRun("e2e-relay-hc-relay", [
      "relay",
      "--secret-key-seed",
      RELAY_SEED,
      "--port",
      "4001",
    ]);
    await waitForLog("e2e-relay-hc-relay", "relay listening address");

    // 2. Start HTTP server (target reachable from listener side)
    await dockerRun(
      "e2e-relay-hc-http",
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
    const relayAddr = `/dns4/e2e-relay-hc-relay/tcp/4001/p2p/${RELAY_PEER_ID}`;
    await dockerRun("e2e-relay-hc-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
      "--relay-address",
      relayAddr,
    ]);
    await waitForLog("e2e-relay-hc-listener", "Relay dial address:");

    // 4. Start dialer with mixed SOCKS5 + HTTP CONNECT proxy
    const circuitAddr = `${relayAddr}/p2p-circuit/p2p/${LISTENER_PEER_ID}`;
    await dockerRun(
      "e2e-relay-hc-dialer",
      [
        "dial",
        "-D",
        "1080",
        circuitAddr,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog("e2e-relay-hc-dialer", "socks5+http proxy listening");

    // 5. Curl through HTTP CONNECT proxy from inside the dialer container
    //    The request goes: dialer:1080 -> tunnel -> listener -> e2e-relay-hc-http:9000
    const body = await dockerExec(
      "e2e-relay-hc-dialer",
      [
        "curl",
        "-sf",
        "--proxytunnel",
        "-x",
        "http://127.0.0.1:1080",
        "http://e2e-relay-hc-http:9000/",
      ],
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
