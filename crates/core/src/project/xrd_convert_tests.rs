use super::*;

fn fixture() -> XrdData {
    XrdData {
        two_theta_deg: vec![3.0, 3.1, 3.2],
        intensity: vec![10.0, 25.0, 12.0],
        attenuation: Some(vec![1.0, 1.0, 2.0]),
        source: "sample.rasx".to_owned(),
        instrument: Some("MiniFlex".to_owned()),
        target: Some("Cu".to_owned()),
        wavelength_angstrom: Some(1.540593),
        voltage_kv: Some(40.0),
        current_ma: Some(15.0),
        scan_step_deg: Some(0.1),
        scan_speed_deg_min: Some(5.0),
    }
}

fn encode(data: &XrdData) -> Vec<u8> {
    let mut bytes = Vec::new();
    write(&mut bytes, data).unwrap();
    bytes
}

fn decode_bytes(bytes: &[u8]) -> Result<XrdData> {
    let mut reader = EntryReader::new(
        std::io::Cursor::new(bytes),
        "test.bin",
        "XRD",
        bytes.len() as u64,
        bytes.len() as u64,
    )?;
    let data = decode(&mut reader)?;
    reader.finish()?;
    Ok(data)
}

#[test]
fn binary_round_trip_preserves_arrays_and_metadata() {
    let original = fixture();
    let decoded = decode_bytes(&encode(&original)).unwrap();

    assert_eq!(decoded.two_theta_deg, original.two_theta_deg);
    assert_eq!(decoded.intensity, original.intensity);
    assert_eq!(decoded.attenuation, original.attenuation);
    assert_eq!(decoded.instrument, original.instrument);
    assert_eq!(decoded.wavelength_angstrom, original.wavelength_angstrom);
}

#[test]
fn binary_decoder_rejects_truncation_and_trailing_bytes() {
    let encoded = encode(&fixture());
    assert!(decode_bytes(&encoded[..encoded.len() - 1]).is_err());

    let mut trailing = encoded;
    trailing.push(0);
    assert!(decode_bytes(&trailing).is_err());
}

#[test]
fn scientific_arrays_are_not_stored_in_metadata_json() {
    let encoded = encode(&fixture());
    let metadata_len = u64::from_le_bytes(encoded[8..16].try_into().unwrap()) as usize;
    let metadata: XrdData = serde_json::from_slice(&encoded[16..16 + metadata_len]).unwrap();

    assert!(metadata.two_theta_deg.is_empty());
    assert!(metadata.intensity.is_empty());
    assert_eq!(metadata.attenuation, Some(Vec::new()));
}

#[test]
fn rejects_implausible_array_length_before_allocation() {
    let mut encoded = encode(&fixture());
    let metadata_len = u64::from_le_bytes(encoded[8..16].try_into().unwrap()) as usize;
    let first_array_len = 16 + metadata_len;
    encoded[first_array_len..first_array_len + 8].copy_from_slice(&u64::MAX.to_le_bytes());

    assert!(decode_bytes(&encoded).is_err());
}

#[test]
fn rejects_aggregate_arrays_over_materialized_limit_before_allocation() {
    let mut encoded = encode(&fixture());
    let metadata_len = u64::from_le_bytes(encoded[8..16].try_into().unwrap()) as usize;
    let first_array_len = 16 + metadata_len;
    let points = ProjectLoadLimits::default().max_materialized_bytes / (3 * 8) + 1;
    encoded[first_array_len..first_array_len + 8].copy_from_slice(&points.to_le_bytes());

    let error = decode_bytes(&encoded).unwrap_err();
    assert!(error.to_string().contains("materialized-data limit"));
}
