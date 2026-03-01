import { assert } from "@std/assert";
import { DIALER_SEED, LISTENER_PEER_ID, LISTENER_SEED } from "./constants.ts";
import {
  dockerLogs,
  dockerRm,
  dockerRun,
  dockerRunInteractive,
  waitForLog,
} from "./helpers.ts";

Deno.test("mDNS netcat: bidirectional data over LAN discovery", async () => {
  const containers = ["e2e-mdns-nc-listener"];
  try {
    // 1. Start listener (no relay — LAN-only mDNS mode)
    await dockerRun("e2e-mdns-nc-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
    ]);
    await waitForLog("e2e-mdns-nc-listener", "LAN dial address:");

    // 2. Dialer sends a marker string through netcat via mDNS
    const marker = `E2E_MDNS_NC_${Date.now()}`;
    await dockerRunInteractive(
      "e2e-mdns-nc-dialer",
      [
        "dial",
        `/mdns/${LISTENER_PEER_ID}`,
        "--secret-key-seed",
        DIALER_SEED,
      ],
      marker + "\n",
    );

    // 3. Verify listener received the marker on stdout (captured by Docker)
    const logs = await dockerLogs("e2e-mdns-nc-listener");
    assert(
      logs.includes(marker),
      `Listener stdout should contain "${marker}".\nActual logs:\n${logs}`,
    );
  } finally {
    await dockerRm(...containers);
  }
});
