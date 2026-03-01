/** Docker image name for e2e tests */
export const IMAGE = "sape-e2e";

/** Docker bridge network — shared by all e2e tests */
export const NETWORK = "sape-e2e-net";

/** Fixed bridge device name so we can disable IGMP snooping */
export const BRIDGE_NAME = "sape-e2e-br";

/**
 * Deterministic secret key seeds and their corresponding peer IDs.
 * seed 1 → relay, seed 2 → listener, seed 3 → dialer
 */
export const RELAY_SEED = "1";
export const LISTENER_SEED = "2";
export const DIALER_SEED = "3";

export const RELAY_PEER_ID =
  "12D3KooWPjceQrSwdWXPyLLeABRXmuqt69Rg3sBYbU1Nft9HyQ6X";
export const LISTENER_PEER_ID =
  "12D3KooWH3uVF6wv47WnArKHk5p6cvgCJEb74UTmxztmQDc298L3";
export const DIALER_PEER_ID =
  "12D3KooWQYhTNQdmr3ArTeUHRYzFg94BKyTkoWBDWez9kSCVe2Xo";
