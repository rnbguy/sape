import { assert } from "@std/assert";
import { DIALER_SEED, LISTENER_PEER_ID, LISTENER_SEED } from "./constants.ts";
import {
  dockerExec,
  dockerLogs,
  dockerRm,
  dockerRun,
  waitForLog,
} from "./helpers.ts";

Deno.test("mDNS socks5: SOCKS5 proxy over LAN discovery", async () => {
  const containers = [
    "e2e-mdns-s5-http",
    "e2e-mdns-s5-listener",
    "e2e-mdns-s5-dialer",
  ];
  try {
    // 1. Start HTTP server (target reachable from listener side)
    await dockerRun(
      "e2e-mdns-s5-http",
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
    await dockerRun("e2e-mdns-s5-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
    ]);
    await waitForLog("e2e-mdns-s5-listener", "LAN dial address:");

    // 3. Start dialer with SOCKS5 proxy
    await dockerRun(
      "e2e-mdns-s5-dialer",
      [
        "dial",
        "-D",
        "1080",
        `/mdns/${LISTENER_PEER_ID}`,
        "--secret-key-seed",
        DIALER_SEED,
      ],
    );
    await waitForLog("e2e-mdns-s5-dialer", "socks5+http proxy listening");

    // 4. Curl through SOCKS5 proxy from inside the dialer container
    //    The request goes: dialer:1080 → tunnel → listener → e2e-mdns-s5-http:9000
    const body = await dockerExec(
      "e2e-mdns-s5-dialer",
      [
        "curl",
        "-sf",
        "--socks5-hostname",
        "127.0.0.1:1080",
        "http://e2e-mdns-s5-http:9000/",
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
