use super::embedded_error::EmbeddedError;
use crate::{FormatError, Result};
use std::cell::Cell;

pub(super) fn decode_embedded(strdata: &str) -> std::result::Result<Vec<u8>, String> {
    decode_embedded_bounded(strdata, crate::MAX_RESOURCE, &Cell::new(crate::MAX_RESOURCE + 1)).map_err(|error| error.to_string())
}

pub(super) fn decode_embedded_bounded(strdata: &str, remaining_bytes: u64, inflation_work: &Cell<u64>) -> std::result::Result<Vec<u8>, EmbeddedError> {
    craft_sbs::embedded::decode_bounded(strdata, crate::MAX_RESOURCE, remaining_bytes, inflation_work)
}

pub(super) fn encode_embedded(bytes: &[u8]) -> Result<String> {
    craft_sbs::embedded::encode(bytes).map_err(FormatError::Io)
}

#[cfg(test)]
pub(super) fn base64_engine() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

#[cfg(test)]
use base64::Engine as _;
#[cfg(test)]
use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_encoder_preserves_embedded_wire_bytes() {
        for len in 0..=513 {
            let bytes: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let mut encoder = flate2::write::ZlibEncoder::new((len as u32).to_be_bytes().to_vec(), flate2::Compression::default());
            encoder.write_all(&bytes).unwrap();
            let raw = encoder.finish().unwrap();

            assert_eq!(encode_embedded(&bytes).unwrap(), base64_engine().encode(&raw));
            assert_eq!(decode_embedded(&encode_embedded(&bytes).unwrap()).unwrap(), bytes);
        }
    }

    #[test]
    fn strict_decoder_policy_is_not_replaced_by_mime_tolerance() {
        assert!(decode_embedded("!!!!").unwrap_err().starts_with("bad base64:"));
        assert!(decode_embedded("AA").unwrap_err().starts_with("bad base64:"));

        let encoded = encode_embedded(b"payload").unwrap();
        let spaced = encoded.chars().map(|c| format!("{c} \n\u{2003}")).collect::<String>();

        assert_eq!(decode_embedded(&spaced).unwrap(), b"payload");
    }

    #[test]
    fn aggregate_budget_bounds_inflation_without_masking_early_corruption() {
        for length in [3_u32, 4] {
            let mut raw = length.to_be_bytes().to_vec();
            raw.push(255);
            let encoded = base64_engine().encode(raw);
            assert!(matches!(decode_embedded_bounded(&encoded, 3, &Cell::new(16)), Err(EmbeddedError::Damaged(_))));
        }
        let encoded = encode_embedded(b"four").unwrap();

        assert!(matches!(decode_embedded_bounded(&encoded, 3, &Cell::new(16)), Err(EmbeddedError::Budget)));
        assert_eq!(decode_embedded_bounded(&encoded, 4, &Cell::new(16)).unwrap(), b"four");
    }

    #[test]
    fn inflation_work_counts_discarded_prefixes_and_stops_before_next_decode() {
        let work = Cell::new(8);
        let encoded = encode_embedded(b"four").unwrap();
        for expected in [4, 0] {
            assert!(matches!(decode_embedded_bounded(&encoded, 3, &work), Err(EmbeddedError::Budget)));
            assert_eq!(work.get(), expected);
        }

        // Exhaustion stops the next resource before Base64 decoding
        assert!(matches!(decode_embedded_bounded("!!!!", 3, &work), Err(EmbeddedError::WorkBudget)));

        let work = Cell::new(2);
        assert!(matches!(decode_embedded_bounded(&encoded, 4, &work), Err(EmbeddedError::WorkBudget)));
        assert_eq!(work.get(), 0);
    }

    #[test]
    fn checksum_errors_charge_output_without_spending_retained_budget() {
        let mut raw = base64_engine().decode(encode_embedded(b"four").unwrap()).unwrap();
        *raw.last_mut().unwrap() ^= 1;
        let work = Cell::new(8);

        assert!(matches!(decode_embedded_bounded(&base64_engine().encode(raw), 4, &work), Err(EmbeddedError::Damaged(_))));
        assert_eq!(work.get(), 4);
    }

    #[test]
    fn proven_length_mismatch_precedes_retained_budget() {
        let mut raw = base64_engine().decode(encode_embedded(b"four").unwrap()).unwrap();
        raw[..4].copy_from_slice(&3_u32.to_be_bytes());
        let work = Cell::new(4);

        assert!(matches!(decode_embedded_bounded(&base64_engine().encode(raw), 3, &work), Err(EmbeddedError::Damaged(_))));
        assert_eq!(work.get(), 0);
    }

    #[test]
    fn retained_assets_release_spare_decompression_capacity() {
        let bytes = vec![17; 2 << 20];
        let encoded = encode_embedded(&bytes).unwrap();

        let decoded = decode_embedded(&encoded).unwrap();

        assert_eq!(decoded, bytes);
        assert!(decoded.capacity() < 2 * decoded.len(), "retained {} bytes for a {}-byte asset", decoded.capacity(), decoded.len());
    }

    #[test]
    fn decompression_requires_the_declared_length() {
        let encoded = encode_embedded(&[1, 2, 3, 4]).unwrap();

        assert_eq!(decode_embedded(&encoded).unwrap(), [1, 2, 3, 4]);

        let mut raw = base64_engine().decode(&encoded).unwrap();
        for length in [1_u32, 5] {
            raw[..4].copy_from_slice(&length.to_be_bytes());

            assert!(decode_embedded(&base64_engine().encode(&raw)).unwrap_err().contains("declared"));
        }

        raw[..4].copy_from_slice(&u32::MAX.to_be_bytes());

        assert!(decode_embedded(&base64_engine().encode(&raw)).unwrap_err().contains("256 MiB limit"));
    }
}
