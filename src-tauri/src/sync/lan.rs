// Phase 3: LAN sync via mDNS discovery + TCP transport
//
// Discovery: Use mdns-sd crate to advertise and discover
//   _sharesticky._tcp.local services on the LAN.
//
// Sync protocol:
//   1. Discover peers via mDNS
//   2. Exchange state vectors (Yrs sync step 1)
//   3. Send missing updates (Yrs sync step 2)
//   4. Apply received updates to local Yrs docs
//
// Only stickies with sharing_tier >= 1 participate in LAN sync.
