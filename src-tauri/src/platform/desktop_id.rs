//! Pure encoding/decoding of virtual desktop identifiers.
//!
//! Windows stores desktop GUIDs as raw 16-byte blobs in the registry
//! (`CurrentVirtualDesktop`, `VirtualDesktopIDs`) and hands them to us as COM
//! `GUID` structs. Both are just the same 16 bytes in mixed-endian layout.
//!
//! This module deliberately depends on **nothing platform-specific** so it can
//! be unit tested on any host. The `windows`-crate types stay in the adapter.

/// A virtual desktop identifier in its canonical string form,
/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
pub type DesktopId = String;

/// Why a desktop id string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopIdError {
    /// The string did not have five hyphen-separated groups.
    MalformedShape(String),
    /// A group was not valid hexadecimal, or had the wrong width.
    InvalidHex(String),
}

impl std::fmt::Display for DesktopIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedShape(s) => write!(f, "malformed desktop id: {s}"),
            Self::InvalidHex(s) => write!(f, "invalid hex in desktop id: {s}"),
        }
    }
}

impl std::error::Error for DesktopIdError {}

/// Format 16 raw bytes as a canonical desktop id.
///
/// The first three groups are little-endian integers; the last eight bytes are
/// in wire order. This mirrors how Windows lays out a `GUID` in memory.
pub fn format_desktop_id(bytes: &[u8; 16]) -> DesktopId {
    let data1 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let data2 = u16::from_le_bytes([bytes[4], bytes[5]]);
    let data3 = u16::from_le_bytes([bytes[6], bytes[7]]);

    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        data1,
        data2,
        data3,
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

/// Parse a canonical desktop id back into its 16 raw bytes.
///
/// Accepts the id with or without surrounding braces, in either case.
pub fn parse_desktop_id(s: &str) -> Result<[u8; 16], DesktopIdError> {
    let trimmed = s.trim().trim_matches(|c| c == '{' || c == '}');
    let groups: Vec<&str> = trimmed.split('-').collect();
    if groups.len() != 5 {
        return Err(DesktopIdError::MalformedShape(s.to_string()));
    }

    // Expected widths, in hex characters, of each hyphen-separated group.
    const WIDTHS: [usize; 5] = [8, 4, 4, 4, 12];
    for (group, width) in groups.iter().zip(WIDTHS) {
        if group.len() != width {
            return Err(DesktopIdError::InvalidHex(s.to_string()));
        }
    }

    let hex = |g: &str| u64::from_str_radix(g, 16).map_err(|_| DesktopIdError::InvalidHex(s.to_string()));

    let data1 = hex(groups[0])? as u32;
    let data2 = hex(groups[1])? as u16;
    let data3 = hex(groups[2])? as u16;

    let mut bytes = [0u8; 16];
    bytes[0..4].copy_from_slice(&data1.to_le_bytes());
    bytes[4..6].copy_from_slice(&data2.to_le_bytes());
    bytes[6..8].copy_from_slice(&data3.to_le_bytes());

    // Groups four and five are stored in wire order, not little-endian.
    let tail: String = format!("{}{}", groups[3], groups[4]);
    for i in 0..8 {
        bytes[8 + i] = u8::from_str_radix(&tail[i * 2..i * 2 + 2], 16)
            .map_err(|_| DesktopIdError::InvalidHex(s.to_string()))?;
    }

    Ok(bytes)
}

/// Split a `VirtualDesktopIDs` registry blob into individual desktop ids.
///
/// The value is a flat concatenation of 16-byte GUIDs. A trailing partial
/// chunk is ignored rather than treated as an error, since a truncated read
/// should degrade to "the desktops we could read" rather than failing outright.
pub fn parse_desktop_id_list(bytes: &[u8]) -> Vec<DesktopId> {
    bytes
        .chunks_exact(16)
        .map(|chunk| {
            let mut guid = [0u8; 16];
            guid.copy_from_slice(chunk);
            format_desktop_id(&guid)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from HKCU\...\Explorer\VirtualDesktops\CurrentVirtualDesktop on a
    // real machine, alongside the id that IVirtualDesktopManager reported for the
    // same desktop. This pins the byte layout against actual Windows behaviour
    // rather than against our own assumptions.
    const REAL_BYTES: [u8; 16] = [
        0x9E, 0x39, 0x9D, 0x3F, // data1, little-endian
        0xCF, 0xC0, // data2, little-endian
        0xD2, 0x41, // data3, little-endian
        0x97, 0x43, // data4, wire order
        0x5A, 0x22, 0x95, 0x63, 0xDE, 0xDA,
    ];
    const REAL_ID: &str = "{3F9D399E-C0CF-41D2-9743-5A229563DEDA}";

    #[test]
    fn formats_registry_bytes_the_way_windows_reports_them() {
        assert_eq!(format_desktop_id(&REAL_BYTES), REAL_ID);
    }

    #[test]
    fn parses_a_canonical_id_back_to_its_registry_bytes() {
        assert_eq!(parse_desktop_id(REAL_ID).unwrap(), REAL_BYTES);
    }

    #[test]
    fn round_trips_arbitrary_bytes() {
        let bytes: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF,
        ];
        assert_eq!(parse_desktop_id(&format_desktop_id(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn round_trips_the_all_zero_id() {
        let bytes = [0u8; 16];
        assert_eq!(format_desktop_id(&bytes), "{00000000-0000-0000-0000-000000000000}");
        assert_eq!(parse_desktop_id(&format_desktop_id(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn accepts_an_id_without_braces() {
        assert_eq!(parse_desktop_id(REAL_ID.trim_matches(|c| c == '{' || c == '}')).unwrap(), REAL_BYTES);
    }

    #[test]
    fn accepts_a_lowercase_id() {
        assert_eq!(parse_desktop_id(&REAL_ID.to_lowercase()).unwrap(), REAL_BYTES);
    }

    #[test]
    fn rejects_an_id_with_the_wrong_number_of_groups() {
        assert!(matches!(
            parse_desktop_id("{3F9D399E-C0CF-41D2-9743}"),
            Err(DesktopIdError::MalformedShape(_))
        ));
    }

    #[test]
    fn rejects_an_id_containing_non_hex_characters() {
        assert!(matches!(
            parse_desktop_id("{ZZZZZZZZ-C0CF-41D2-9743-5A229563DEDA}"),
            Err(DesktopIdError::InvalidHex(_))
        ));
    }

    #[test]
    fn rejects_an_id_whose_groups_are_the_wrong_width() {
        assert!(matches!(
            parse_desktop_id("{3F9D399E-C0CF-41D2-97-435A229563DEDA}"),
            Err(DesktopIdError::InvalidHex(_))
        ));
    }

    #[test]
    fn reads_an_empty_desktop_list() {
        assert_eq!(parse_desktop_id_list(&[]), Vec::<DesktopId>::new());
    }

    #[test]
    fn reads_a_single_desktop_from_a_list() {
        assert_eq!(parse_desktop_id_list(&REAL_BYTES), vec![REAL_ID.to_string()]);
    }

    #[test]
    fn reads_several_desktops_in_order() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&REAL_BYTES);
        blob.extend_from_slice(&[0u8; 16]);

        assert_eq!(
            parse_desktop_id_list(&blob),
            vec![
                REAL_ID.to_string(),
                "{00000000-0000-0000-0000-000000000000}".to_string(),
            ]
        );
    }

    #[test]
    fn ignores_a_trailing_partial_desktop_entry() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&REAL_BYTES);
        blob.extend_from_slice(&[0xAB; 7]); // truncated tail

        assert_eq!(parse_desktop_id_list(&blob), vec![REAL_ID.to_string()]);
    }
}
