use std::panic::catch_unwind;

use crate::origin::{OriginCell, OriginError, OriginLimits, read_origin};

const SIGNATURE: &[u8] = b"CPYA 4.3268 195 W64 #\n";
const GLOBAL_HEADER_LEN: usize = 115;
const DATA_HEADER_LEN: usize = 147;
const EMPTY_F64: f64 = -1.23456789E-300;

fn push_block(bytes: &mut Vec<u8>, payload: Option<&[u8]>) {
    let payload = payload.unwrap_or_default();
    bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
    bytes.push(b'\n');
    if !payload.is_empty() {
        bytes.extend_from_slice(payload);
        bytes.push(b'\n');
    }
}

fn modern_header(name: &str, row_count: u32) -> [u8; DATA_HEADER_LEN] {
    let mut header = [0_u8; DATA_HEADER_LEN];
    header[0x16..0x18].copy_from_slice(&0x6121_u16.to_le_bytes());
    header[0x18] = 0x03;
    header[0x19..0x1d].copy_from_slice(&row_count.to_le_bytes());
    header[0x1d..0x21].copy_from_slice(&0_u32.to_le_bytes());
    header[0x21..0x25].copy_from_slice(&row_count.to_le_bytes());
    header[0x3d] = 10;
    header[0x3f] = 0x10;
    header[0x58..0x58 + name.len()].copy_from_slice(name.as_bytes());
    header[0x71..0x73].copy_from_slice(&0x10ca_u16.to_le_bytes());
    header
}

fn numeric_slots(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| [0, 0].into_iter().chain(value.to_le_bytes()))
        .collect()
}

fn push_window(bytes: &mut Vec<u8>, name: &str) {
    let mut header = [0_u8; 27];
    header[2..2 + name.len()].copy_from_slice(name.as_bytes());
    push_block(bytes, Some(&header));
    push_block(bytes, None);
}

struct ModernProject {
    bytes: Vec<u8>,
    window_list_end: usize,
    final_window_terminator_start: usize,
}

fn modern_project(records: &[(&str, &[f64])], windows: &[&str]) -> ModernProject {
    let mut bytes = SIGNATURE.to_vec();
    let mut global = [0_u8; GLOBAL_HEADER_LEN];
    global[0x1b..0x23].copy_from_slice(&9.510195_f64.to_le_bytes());
    push_block(&mut bytes, Some(&global));
    push_block(&mut bytes, None);

    for (name, values) in records {
        push_block(
            &mut bytes,
            Some(&modern_header(name, u32::try_from(values.len()).unwrap())),
        );
        push_block(&mut bytes, Some(&numeric_slots(values)));
        push_block(&mut bytes, None);
    }
    push_block(&mut bytes, None);

    for name in windows {
        push_window(&mut bytes, name);
    }
    let final_window_terminator_start = bytes.len();
    push_block(&mut bytes, None);
    let window_list_end = bytes.len();

    // This payload deliberately resembles a dataset name. A windows-only
    // parser must report and ignore it rather than scan it for table markers.
    bytes.extend_from_slice(b"opaque Book1_FAKE project tail\0\xff");
    ModernProject {
        bytes,
        window_list_end,
        final_window_terminator_start,
    }
}

#[test]
fn modern_assembles_worksheets_and_reports_the_opaque_tail() {
    let fixture = modern_project(
        &[("Book1_A", &[0.05, 0.10]), ("Book1_B", &[1.5, 2.5])],
        &["Book1", "Graph1"],
    );
    let project = read_origin(&fixture.bytes, OriginLimits::default()).unwrap();

    assert_eq!(project.workbooks.len(), 1);
    let worksheet = &project.workbooks[0].worksheets[0];
    assert_eq!(project.workbooks[0].name, "Book1");
    assert_eq!(worksheet.row_count, 2);
    assert_eq!(worksheet.columns.len(), 2);
    assert_eq!(worksheet.columns[0].name, "A");
    assert_eq!(worksheet.columns[1].name, "B");
    assert_eq!(
        worksheet.columns[0].cells,
        [OriginCell::Float(0.05), OriginCell::Float(0.10)]
    );
    assert!(project.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("remaining Origin 9.51 project objects")
    }));
    assert!(!worksheet.columns.iter().any(|column| column.name == "FAKE"));
}

#[test]
fn modern_accepts_an_exact_window_boundary_without_inventing_a_tail() {
    let fixture = modern_project(&[("Book1_A", &[1.0])], &["Book1"]);
    let project = read_origin(
        &fixture.bytes[..fixture.window_list_end],
        OriginLimits::default(),
    )
    .unwrap();
    assert_eq!(project.workbooks.len(), 1);
    assert!(!project.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("remaining Origin 9.51 project objects")
    }));
}

#[test]
fn modern_rejects_every_prefix_before_the_window_list_terminator() {
    let fixture = modern_project(&[("Book1_A", &[1.0])], &["Book1"]);
    for end in 0..fixture.window_list_end {
        let outcome = catch_unwind(|| read_origin(&fixture.bytes[..end], OriginLimits::default()));
        assert!(outcome.is_ok(), "prefix {end} panicked");
        assert!(outcome.unwrap().is_err(), "prefix {end} succeeded");
    }
}

#[test]
fn modern_rejects_a_missing_window_list_terminator() {
    let fixture = modern_project(&[("Book1_A", &[1.0])], &["Book1"]);
    let bytes = &fixture.bytes[..fixture.final_window_terminator_start];
    assert!(matches!(
        read_origin(bytes, OriginLimits::default()),
        Err(OriginError::Truncated { .. })
    ));
}

#[test]
fn modern_enforces_the_window_record_limit_before_association() {
    let fixture = modern_project(&[("Book1_A", &[1.0])], &["Book1", "Graph1"]);
    let limits = OriginLimits {
        max_window_records: 1,
        ..OriginLimits::default()
    };
    assert!(matches!(
        read_origin(&fixture.bytes, limits),
        Err(OriginError::LimitExceeded {
            resource: "window records",
            limit: 1,
            actual: 2,
        })
    ));
}

#[test]
fn modern_rejects_ambiguous_or_absent_worksheet_associations() {
    for fixture in [
        modern_project(&[("Book1_A", &[1.0])], &["Book1", "Book1"]),
        modern_project(&[], &["Graph1"]),
    ] {
        assert_eq!(
            read_origin(&fixture.bytes, OriginLimits::default()).unwrap_err(),
            OriginError::NoSupportedWorksheet
        );
    }
}

#[test]
fn modern_empty_columns_remain_null_after_worksheet_padding() {
    let fixture = modern_project(
        &[("Book1_A", &[1.0, 2.0]), ("Book1_B", &[EMPTY_F64])],
        &["Book1"],
    );
    let project = read_origin(&fixture.bytes, OriginLimits::default()).unwrap();
    let worksheet = &project.workbooks[0].worksheets[0];
    assert_eq!(worksheet.row_count, 2);
    assert_eq!(worksheet.columns[1].cells, [OriginCell::Null]);
}
