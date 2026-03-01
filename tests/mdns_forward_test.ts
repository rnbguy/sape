import { assert } from "jsr:@std/assert";
import { DIALER_SEED, LISTENER_PEER_ID, LISTENER_SEED } from "./constants.ts";
import {
  dockerExec,
  dockerLogs,
  dockerRm,
  dockerRun,
  waitForLog,
} from "./helpers.ts";

Deno.test("mDNS local-forward: TCP forwarding over LAN discovery", async () => {
  const containers = [
    "e2e-mdns-fwd-http",
    "e2e-mdns-fwd-listener",
    "e2e-mdns-fwd-dialer",
  ];
  try {
    // 1. Start a simple HTTP server as the forwarding target
    await dockerRun(
      "e2e-mdns-fwd-http",
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

    // 2. Start listener (no relay — mDNS only)
    await dockerRun("e2e-mdns-fwd-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
    ]);
    await waitForLog("e2e-mdns-fwd-listener", "LAN dial address:");

    // 3. Start dialer with local-forward
    //    Target resolves on listener side via Docker DNS
    await dockerRun(
      "e2e-mdns-fwd-dialer",
      [
        "dial",
        "-L",
        "8080:e2e-mdns-fwd-http:9000",
        `/mdns/${LISTENER_PEER_ID}`,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog("e2e-mdns-fwd-dialer", "local forward is listening");

    // 4. Curl from inside the dialer container (bound to 127.0.0.1:8080)
    const body = await dockerExec(
      "e2e-mdns-fwd-dialer",
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
