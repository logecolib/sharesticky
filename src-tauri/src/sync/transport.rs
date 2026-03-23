// Phase 3: Transport abstraction layer
//
// Provides a unified interface for different sync transports:
// - LAN (mDNS + TCP) - Phase 3
// - WebSocket relay - Future
// - WebRTC - Future
//
// The transport trait will define:
// - connect(peer) -> Connection
// - send(connection, message)
// - receive(connection) -> message
// - disconnect(connection)
