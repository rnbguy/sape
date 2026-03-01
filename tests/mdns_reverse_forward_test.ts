import { assert } from "jsr:@std/assert";
import { DIALER_SEED, LISTENER_PEER_ID, LISTENER_SEED } from "./constants.ts";
import {
  dockerExec,
  dockerLogs,
  dockerRm,
  dockerRun,
  waitForLog,
} from "./helpers.ts";

Deno.test("mDNS reverse-forward: TCP reverse forwarding over LAN discovery", async () => {
  const containers = [
    "e2e-mdns-rfwd-http",
    "e2e-mdns-rfwd-listener",
    "e2e-mdns-rfwd-dialer",
  ];
  try {
    // 1. Start HTTP server (target on dialer's network)
    await dockerRun(
      "e2e-mdns-rfwd-http",
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

    // 2. Start listener (mDNS only)
    await dockerRun("e2e-mdns-rfwd-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
    ]);
    await waitForLog("e2e-mdns-rfwd-listener", "LAN dial address:");

    // 3. Start dialer with reverse-forward
    //    Binds 9090 on LISTENER side, forwards to e2e-mdns-rfwd-http:9000 (dialer side)
    await dockerRun(
      "e2e-mdns-rfwd-dialer",
      [
        "dial",
        "-R",
        "9090:e2e-mdns-rfwd-http:9000",
        `/mdns/${LISTENER_PEER_ID}`,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog(
      "e2e-mdns-rfwd-dialer",
      "reverse forward request accepted",
    );

    // 4. Curl from inside the LISTENER container to the reverse-forwarded port
    const body = await dockerExec(
      "e2e-mdns-rfwd-listener",
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
