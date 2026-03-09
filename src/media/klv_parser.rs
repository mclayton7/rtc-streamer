use serde::{Deserialize, Serialize};

/// MISB ST 0601 Universal Label key (16 bytes).
const MISB_ST0601_UL: [u8; 16] = [
    0x06, 0x0E, 0x2B, 0x34, 0x02, 0x0B, 0x01, 0x01,
    0x0E, 0x01, 0x03, 0x01, 0x01, 0x00, 0x00, 0x00,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KlvField {
    pub tag: u8,
    pub name: String,
    pub value: String,
    pub unit: Option<String>,
}

/// Parse a MISB ST 0601 KLV packet from raw bytes.
///
/// Returns `None` if the 16-byte Universal Label key does not match (not MISB KLV).
pub fn parse_klv_packet(data: &[u8]) -> Option<Vec<KlvField>> {
    if data.len() < 16 {
        return None;
    }
    if data[..16] != MISB_ST0601_UL {
        return None;
    }

    let (length, header_len) = parse_ber_length(&data[16..])?;
    let payload_start = 16 + header_len;
    if payload_start + length > data.len() {
        return None;
    }

    let payload = &data[payload_start..payload_start + length];
    let mut fields = Vec::new();
    let mut i = 0;

    while i < payload.len() {
        let tag = payload[i];
        i += 1;

        if i >= payload.len() {
            break;
        }

        let (len, len_bytes) = match parse_ber_length(&payload[i..]) {
            Some(v) => v,
            None => break,
        };
        i += len_bytes;

        if i + len > payload.len() {
            break;
        }

        let value_bytes = &payload[i..i + len];
        i += len;

        fields.push(decode_tag(tag, value_bytes));
    }

    Some(fields)
}

/// Parse BER-encoded length (short form or long form with 1–2 length bytes).
/// Returns `(length, bytes_consumed)` or `None` if malformed.
fn parse_ber_length(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    if first & 0x80 == 0 {
        Some((first as usize, 1))
    } else if first == 0x81 {
        if data.len() < 2 {
            return None;
        }
        Some((data[1] as usize, 2))
    } else if first == 0x82 {
        if data.len() < 3 {
            return None;
        }
        Some((((data[1] as usize) << 8) | data[2] as usize, 3))
    } else {
        None
    }
}

fn decode_tag(tag: u8, data: &[u8]) -> KlvField {
    match tag {
        2 => {
            let ts = read_u64(data).unwrap_or(0);
            KlvField {
                tag,
                name: "Precision Time Stamp".into(),
                value: format_unix_micros(ts),
                unit: None,
            }
        }
        3 => KlvField {
            tag,
            name: "Mission ID".into(),
            value: String::from_utf8_lossy(data).into_owned(),
            unit: None,
        },
        4 => KlvField {
            tag,
            name: "Platform Tail Number".into(),
            value: String::from_utf8_lossy(data).into_owned(),
            unit: None,
        },
        5 => {
            let deg = read_u16(data).unwrap_or(0) as f64 * 360.0 / 65535.0;
            KlvField {
                tag,
                name: "Platform Heading".into(),
                value: format!("{:.2}", deg),
                unit: Some("°".into()),
            }
        }
        6 => {
            let deg = read_i16(data).unwrap_or(0) as f64 * 20.0 / 32767.0;
            KlvField {
                tag,
                name: "Platform Pitch".into(),
                value: format!("{:.2}", deg),
                unit: Some("°".into()),
            }
        }
        7 => {
            let deg = read_i16(data).unwrap_or(0) as f64 * 50.0 / 32767.0;
            KlvField {
                tag,
                name: "Platform Roll".into(),
                value: format!("{:.2}", deg),
                unit: Some("°".into()),
            }
        }
        13 => {
            let deg = read_i32(data).unwrap_or(0) as f64 * 90.0 / 2_147_483_647.0;
            KlvField {
                tag,
                name: "Sensor Latitude".into(),
                value: format!("{:.6}", deg),
                unit: Some("°".into()),
            }
        }
        14 => {
            let deg = read_i32(data).unwrap_or(0) as f64 * 180.0 / 2_147_483_647.0;
            KlvField {
                tag,
                name: "Sensor Longitude".into(),
                value: format!("{:.6}", deg),
                unit: Some("°".into()),
            }
        }
        15 => {
            let alt = read_u16(data).unwrap_or(0) as f64 * 19900.0 / 65535.0 - 900.0;
            KlvField {
                tag,
                name: "Sensor Altitude".into(),
                value: format!("{:.1}", alt),
                unit: Some("m".into()),
            }
        }
        16 => {
            let deg = read_u16(data).unwrap_or(0) as f64 * 180.0 / 65535.0;
            KlvField {
                tag,
                name: "Sensor H-FoV".into(),
                value: format!("{:.2}", deg),
                unit: Some("°".into()),
            }
        }
        17 => {
            let deg = read_u16(data).unwrap_or(0) as f64 * 180.0 / 65535.0;
            KlvField {
                tag,
                name: "Sensor V-FoV".into(),
                value: format!("{:.2}", deg),
                unit: Some("°".into()),
            }
        }
        23 => {
            let deg = read_i32(data).unwrap_or(0) as f64 * 90.0 / 2_147_483_647.0;
            KlvField {
                tag,
                name: "Frame Center Lat".into(),
                value: format!("{:.6}", deg),
                unit: Some("°".into()),
            }
        }
        24 => {
            let deg = read_i32(data).unwrap_or(0) as f64 * 180.0 / 2_147_483_647.0;
            KlvField {
                tag,
                name: "Frame Center Lon".into(),
                value: format!("{:.6}", deg),
                unit: Some("°".into()),
            }
        }
        65 => KlvField {
            tag,
            name: "UAS LS Version".into(),
            value: read_u16(data).unwrap_or(0).to_string(),
            unit: None,
        },
        _ => {
            let hex = data
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            KlvField {
                tag,
                name: format!("Tag {}", tag),
                value: hex,
                unit: None,
            }
        }
    }
}

fn read_u16(data: &[u8]) -> Option<u16> {
    if data.len() < 2 {
        return None;
    }
    Some(u16::from_be_bytes([data[0], data[1]]))
}

fn read_i16(data: &[u8]) -> Option<i16> {
    if data.len() < 2 {
        return None;
    }
    Some(i16::from_be_bytes([data[0], data[1]]))
}

fn read_i32(data: &[u8]) -> Option<i32> {
    if data.len() < 4 {
        return None;
    }
    Some(i32::from_be_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64(data: &[u8]) -> Option<u64> {
    if data.len() < 8 {
        return None;
    }
    Some(u64::from_be_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
}

/// Format Unix microseconds as an ISO 8601 UTC datetime string.
fn format_unix_micros(micros: u64) -> String {
    let secs = micros / 1_000_000;
    let us = micros % 1_000_000;
    let days = secs / 86400;
    let tod = secs % 86400;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    let s = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        year, month, day, h, m, s, us
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a minimal valid MISB KLV packet from a raw tags payload slice.
    fn make_klv(tags_payload: &[u8]) -> Vec<u8> {
        let mut pkt = MISB_ST0601_UL.to_vec();
        let len = tags_payload.len();
        if len <= 127 {
            pkt.push(len as u8);
        } else if len <= 255 {
            pkt.push(0x81);
            pkt.push(len as u8);
        } else {
            pkt.push(0x82);
            pkt.push((len >> 8) as u8);
            pkt.push((len & 0xFF) as u8);
        }
        pkt.extend_from_slice(tags_payload);
        pkt
    }

    // --- parse_ber_length ---

    #[test]
    fn ber_length_short_form() {
        assert_eq!(parse_ber_length(&[0x05]), Some((5, 1)));
        assert_eq!(parse_ber_length(&[0x00]), Some((0, 1)));
        assert_eq!(parse_ber_length(&[0x7F]), Some((127, 1)));
    }

    #[test]
    fn ber_length_0x81() {
        assert_eq!(parse_ber_length(&[0x81, 0xC8]), Some((200, 2)));
        assert_eq!(parse_ber_length(&[0x81]), None);
    }

    #[test]
    fn ber_length_0x82() {
        assert_eq!(parse_ber_length(&[0x82, 0x01, 0x00]), Some((256, 3)));
        assert_eq!(parse_ber_length(&[0x82, 0x01]), None);
    }

    #[test]
    fn ber_length_unsupported() {
        assert_eq!(parse_ber_length(&[0x83, 0x00, 0x00, 0x00]), None);
        assert_eq!(parse_ber_length(&[0xFF]), None);
    }

    #[test]
    fn ber_length_empty() {
        assert_eq!(parse_ber_length(&[]), None);
    }

    // --- parse_klv_packet ---

    #[test]
    fn parse_klv_wrong_key() {
        let wrong_key = [0x00u8; 16];
        let mut pkt = wrong_key.to_vec();
        pkt.push(0x00); // length 0
        assert!(parse_klv_packet(&pkt).is_none());
    }

    #[test]
    fn parse_klv_too_short() {
        assert!(parse_klv_packet(&[0x00; 15]).is_none());
        assert!(parse_klv_packet(&[]).is_none());
    }

    #[test]
    fn parse_klv_tag4_string() {
        // tag=4, length=4, value="TEST"
        let tags = [0x04, 0x04, 0x54, 0x45, 0x53, 0x54];
        let pkt = make_klv(&tags);
        let fields = parse_klv_packet(&pkt).expect("should parse");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].value, "TEST");
    }

    #[test]
    fn parse_klv_lat_max() {
        // tag=13, length=4, value=i32::MAX big-endian
        let v = i32::MAX.to_be_bytes();
        let tags = [0x0D, 0x04, v[0], v[1], v[2], v[3]];
        let pkt = make_klv(&tags);
        let fields = parse_klv_packet(&pkt).expect("should parse");
        assert_eq!(fields[0].value, "90.000000");
    }

    #[test]
    fn parse_klv_lon_max() {
        // tag=14, length=4, value=i32::MAX big-endian
        let v = i32::MAX.to_be_bytes();
        let tags = [0x0E, 0x04, v[0], v[1], v[2], v[3]];
        let pkt = make_klv(&tags);
        let fields = parse_klv_packet(&pkt).expect("should parse");
        assert_eq!(fields[0].value, "180.000000");
    }

    // --- decode_tag ---

    #[test]
    fn decode_tag_heading_full_scale() {
        let field = decode_tag(5, &[0xFF, 0xFF]);
        assert_eq!(field.value, "360.00");
        assert_eq!(field.unit.as_deref(), Some("°"));
    }

    #[test]
    fn decode_tag_unknown_hex() {
        let field = decode_tag(99, &[0xDE, 0xAD]);
        assert_eq!(field.name, "Tag 99");
        assert_eq!(field.value, "de ad");
        assert!(field.unit.is_none());
    }
}

/// Convert days since Unix epoch to (year, month, day).
///
/// Uses the algorithm from http://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(days: u64) -> (i32, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}
