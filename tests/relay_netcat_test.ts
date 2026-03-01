import { assert } from "jsr:@std/assert";
import {
  DIALER_SEED,
  LISTENER_PEER_ID,
  LISTENER_SEED,
  RELAY_PEER_ID,
  RELAY_SEED,
} from "./constants.ts";
import {
  dockerLogs,
  dockerRm,
  dockerRun,
  dockerRunInteractive,
  waitForLog,
} from "./helpers.ts";

Deno.test("relay netcat: bidirectional data over relay circuit", async () => {
  const containers = [
    "e2e-relay-nc-relay",
    "e2e-relay-nc-listener",
  ];
  try {
    // 1. Start relay server
    await dockerRun("e2e-relay-nc-relay", [
      "relay",
      "--secret-key-seed",
      RELAY_SEED,
      "--port",
      "4001",
    ]);
    await waitForLog("e2e-relay-nc-relay", "relay listening address");

    // 2. Start listener with relay
    const relayAddr =
      `/dns4/e2e-relay-nc-relay/tcp/4001/p2p/${RELAY_PEER_ID}`;
    await dockerRun("e2e-relay-nc-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
      "--relay-address",
      relayAddr,
    ]);
    await waitForLog("e2e-relay-nc-listener", "Relay dial address:");

    // 3. Dialer sends a marker through netcat via relay circuit
    const circuitAddr =
      `${relayAddr}/p2p-circuit/p2p/${LISTENER_PEER_ID}`;
    const marker = `E2E_RELAY_NC_${Date.now()}`;
    await dockerRunInteractive(
      "e2e-relay-nc-dialer",
      [
        "dial",
        circuitAddr,
        "--secret-key-seed",
        DIALER_SEED,
      ],
      marker + "\n",
    );

    // 4. Verify listener received the marker
    const logs = await dockerLogs("e2e-relay-nc-listener");
    assert(
      logs.includes(marker),
      `Listener stdout should contain "${marker}".\nActual logs:\n${logs}`,
    );
  } finally {
    await dockerRm(...containers);
  }
});
