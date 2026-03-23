// Phase 3+: Sharing commands
//
// Planned commands:
// - share_sticky(id, tier) -> ShareInfo
// - accept_share(share_key) -> Sticky
// - list_shared_stickies() -> Vec<SharedStickyInfo>
// - revoke_share(id)
//
// Sharing tiers:
//   0 = private (no sharing)
//   1 = LAN only (mDNS discovery)
//   2 = cloud relay (future)
