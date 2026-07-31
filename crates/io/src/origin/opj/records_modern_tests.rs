use std::panic::catch_unwind;

use super::{DecodedColumnRecord, decode_column_record};
use crate::origin::{
    OriginCell, OriginColumnType, OriginError, OriginLimits, OriginProfile, OriginResourceUsage,
};

const HEADER_LEN: usize = 147;
const TYPE_OFFSET: usize = 0x16;
const SECONDARY_OFFSET: usize = 0x18;
const TOTAL_ROWS_OFFSET: usize = 0x19;
const FIRST_ROW_OFFSET: usize = 0x1d;
const LAST_ROW_OFFSET: usize = 0x21;
const WIDTH_OFFSET: usize = 0x3d;
const STORAGE_FLAG_OFFSET: usize = 0x3f;
const NAME_OFFSET: usize = 0x58;
const TERTIARY_OFFSET: usize = 0x71;
const EMPTY_F64: f64 = -1.23456789E-300;

#[derive(Clone, Copy)]
struct ModernHeader {
    data_type: u16,
    secondary: u8,
    total_rows: u32,
    first_row: u32,
    last_row: u32,
    width: u8,
    storage_flag: u8,
    tertiary: u16,
}

impl Default for ModernHeader {
    fn default() -> Self {
        Self {
            data_type: 0x6121,
            secondary: 0x03,
            total_rows: 2,
            first_row: 0,
            last_row: 2,
            width: 10,
            storage_flag: 0x10,
            tertiary: 0x10ca,
        }
    }
}

fn header(spec: ModernHeader) -> Vec<u8> {
    let mut bytes = vec![0_u8; HEADER_LEN];
    bytes[TYPE_OFFSET..TYPE_OFFSET + 2].copy_from_slice(&spec.data_type.to_le_bytes());
    bytes[SECONDARY_OFFSET] = spec.secondary;
    bytes[TOTAL_ROWS_OFFSET..TOTAL_ROWS_OFFSET + 4].copy_from_slice(&spec.total_rows.to_le_bytes());
    bytes[FIRST_ROW_OFFSET..FIRST_ROW_OFFSET + 4].copy_from_slice(&spec.first_row.to_le_bytes());
    bytes[LAST_ROW_OFFSET..LAST_ROW_OFFSET + 4].copy_from_slice(&spec.last_row.to_le_bytes());
    bytes[WIDTH_OFFSET] = spec.width;
    bytes[STORAGE_FLAG_OFFSET] = spec.storage_flag;
    bytes[NAME_OFFSET..NAME_OFFSET + 7].copy_from_slice(b"Book1_A");
    bytes[TERTIARY_OFFSET..TERTIARY_OFFSET + 2].copy_from_slice(&spec.tertiary.to_le_bytes());
    bytes
}

fn slots(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| [0, 0].into_iter().chain(value.to_le_bytes()))
        .collect()
}

fn decode(header: &[u8], content: &[u8]) -> Result<DecodedColumnRecord, OriginError> {
    let mut usage = OriginResourceUsage::default();
    decode_column_record(
        OriginProfile::Origin9V951,
        header,
        Some(content),
        &OriginLimits::default(),
        &mut usage,
    )
}

#[test]
fn modern_decodes_only_the_observed_numeric_field_combinations() {
    let observed = [
        (0x5121, 0x20, 0x10c8),
        (0x5121, 0x30, 0x10c8),
        (0x5121, 0x30, 0x10ca),
        (0x6121, 0x00, 0x11ca),
        (0x6121, 0x10, 0x10c8),
        (0x6121, 0x10, 0x10c9),
        (0x6121, 0x10, 0x10ca),
    ];
    for (data_type, storage_flag, tertiary) in observed {
        let decoded = decode(
            &header(ModernHeader {
                data_type,
                storage_flag,
                tertiary,
                ..ModernHeader::default()
            }),
            &slots(&[0.05, 20.0]),
        )
        .unwrap();
        assert_eq!(decoded.dataset_name, "Book1_A");
        assert_eq!(decoded.column_type, OriginColumnType::Float);
        assert_eq!(
            decoded.cells,
            vec![OriginCell::Float(0.05), OriginCell::Float(20.0)]
        );
    }
}

#[test]
fn modern_rejects_unobserved_cross_product_of_verified_fields() {
    let result = decode(
        &header(ModernHeader {
            data_type: 0x5121,
            storage_flag: 0x00,
            tertiary: 0x11ca,
            ..ModernHeader::default()
        }),
        &slots(&[0.05, 20.0]),
    );

    assert!(matches!(
        result,
        Err(OriginError::UnsupportedFeature { .. })
    ));
}

#[test]
fn modern_maps_the_verified_missing_sentinel_to_null() {
    let decoded = decode(&header(ModernHeader::default()), &slots(&[EMPTY_F64, 1.0])).unwrap();
    assert_eq!(
        decoded.cells,
        vec![OriginCell::Null, OriginCell::Float(1.0)]
    );
}

#[test]
fn modern_retains_an_empty_column_with_validated_storage() {
    for secondary in [0x01, 0x03] {
        let spec = ModernHeader {
            secondary,
            total_rows: 3,
            last_row: 0,
            storage_flag: 0x00,
            tertiary: 0x10ca,
            ..ModernHeader::default()
        };
        let decoded = decode(&header(spec), &slots(&[EMPTY_F64; 3])).unwrap();
        assert!(decoded.cells.is_empty());
        assert_eq!(decoded.first_row, 0);
        assert_eq!(decoded.last_row_exclusive, 0);
    }
}

#[test]
fn modern_rejects_text_discriminator_without_guessing() {
    let mut content = slots(&[1.0, 2.0]);
    content[0] = 1;
    assert!(matches!(
        decode(&header(ModernHeader::default()), &content),
        Err(OriginError::UnsupportedFeature { .. })
    ));
}

#[test]
fn modern_rejects_nonzero_reserved_prefix_as_corrupt() {
    let mut content = slots(&[1.0, 2.0]);
    content[1] = 1;
    assert!(matches!(
        decode(&header(ModernHeader::default()), &content),
        Err(OriginError::CorruptStructure { .. })
    ));
}

#[test]
fn modern_rejects_unobserved_header_fields() {
    let cases = [
        ModernHeader {
            data_type: 0x6001,
            ..ModernHeader::default()
        },
        ModernHeader {
            secondary: 0x01,
            ..ModernHeader::default()
        },
        ModernHeader {
            width: 8,
            ..ModernHeader::default()
        },
        ModernHeader {
            storage_flag: 0x40,
            ..ModernHeader::default()
        },
        ModernHeader {
            tertiary: 0x10e8,
            ..ModernHeader::default()
        },
    ];
    for spec in cases {
        assert!(matches!(
            decode(&header(spec), &slots(&[1.0, 2.0])),
            Err(OriginError::UnsupportedFeature { .. })
        ));
    }
}

#[test]
fn modern_rejects_wrong_header_and_content_lengths() {
    let complete_header = header(ModernHeader::default());
    let complete_content = slots(&[1.0, 2.0]);
    for length in [146, 148] {
        let mut wrong = complete_header.clone();
        wrong.resize(length, 0);
        assert!(decode(&wrong, &complete_content).is_err());
    }
    assert!(decode(&complete_header, &complete_content[..19]).is_err());
    let mut extra = complete_content.clone();
    extra.push(0);
    assert!(decode(&complete_header, &extra).is_err());
}

#[test]
fn modern_rejects_invalid_geometry_and_configured_limits() {
    let invalid = header(ModernHeader {
        first_row: 2,
        last_row: 1,
        ..ModernHeader::default()
    });
    assert!(matches!(
        decode(&invalid, &slots(&[1.0, 2.0])),
        Err(OriginError::CorruptStructure { .. })
    ));

    let mut usage = OriginResourceUsage::default();
    let limits = OriginLimits {
        max_rows_per_column: 1,
        ..OriginLimits::default()
    };
    let content = slots(&[1.0, 2.0]);
    assert!(matches!(
        decode_column_record(
            OriginProfile::Origin9V951,
            &header(ModernHeader::default()),
            Some(&content),
            &limits,
            &mut usage,
        ),
        Err(OriginError::LimitExceeded {
            resource: "rows per column",
            ..
        })
    ));

    let mut usage = OriginResourceUsage::default();
    let limits = OriginLimits {
        max_cells: 1,
        ..OriginLimits::default()
    };
    assert!(matches!(
        decode_column_record(
            OriginProfile::Origin9V951,
            &header(ModernHeader::default()),
            Some(&content),
            &limits,
            &mut usage,
        ),
        Err(OriginError::LimitExceeded {
            resource: "cells",
            ..
        })
    ));
}

#[test]
fn modern_every_truncated_prefix_returns_an_error_without_panicking() {
    let complete_header = header(ModernHeader::default());
    let complete_content = slots(&[1.0, 2.0]);
    for end in 0..complete_header.len() {
        let outcome = catch_unwind(|| decode(&complete_header[..end], &complete_content));
        assert!(outcome.is_ok(), "header prefix {end} panicked");
        assert!(outcome.unwrap().is_err(), "header prefix {end} succeeded");
    }
    for end in 0..complete_content.len() {
        let outcome = catch_unwind(|| decode(&complete_header, &complete_content[..end]));
        assert!(outcome.is_ok(), "content prefix {end} panicked");
        assert!(outcome.unwrap().is_err(), "content prefix {end} succeeded");
    }
}
