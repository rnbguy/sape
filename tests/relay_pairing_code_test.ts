import { assert } from "jsr:@std/assert";
import {
  DIALER_SEED,
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

Deno.test("relay pairing-code: netcat data over relay pairing code", async () => {
  const pairingCode = "42-river-ocean";
  const containers = [
    "e2e-relay-code-relay",
    "e2e-relay-code-listener",
  ];
  try {
    // 1. Start relay server
    await dockerRun("e2e-relay-code-relay", [
      "relay",
      "--secret-key-seed",
      RELAY_SEED,
      "--port",
      "4001",
    ]);
    await waitForLog("e2e-relay-code-relay", "relay listening address");

    // 2. Start listener with relay and pairing code
    const relayAddr =
      `/dns4/e2e-relay-code-relay/tcp/4001/p2p/${RELAY_PEER_ID}`;
    await dockerRun("e2e-relay-code-listener", [
      "listen",
      "--secret-key-seed",
      LISTENER_SEED,
      "--relay-address",
      relayAddr,
      "--code",
      pairingCode,
    ]);
    await waitForLog("e2e-relay-code-listener", `Pairing code: ${pairingCode}`);
    await waitForLog(
      "e2e-relay-code-listener",
      "rendezvous registration accepted",
    );

    // 3. Dialer sends a marker through pairing code rendezvous
    const marker = `E2E_RELAY_CODE_${Date.now()}`;
    const result = await dockerRunInteractive(
      "e2e-relay-code-dialer",
      [
        "dial",
        pairingCode,
        "--secret-key-seed",
        DIALER_SEED,
        "--relay-address",
        relayAddr,
      ],
      marker + "\n",
    );
    assert(
      result.code === 0,
      `Dialer should exit successfully.\nOutput:\n${result.combined}`,
    );

    // 4. Verify listener received the marker
    await waitForLog("e2e-relay-code-listener", marker);
    const logs = await dockerLogs("e2e-relay-code-listener");
    assert(
      logs.includes(marker),
      `Listener stdout should contain "${marker}".\nActual logs:\n${logs}`,
    );
  } finally {
    await dockerRm(...containers);
  }
});
