use super::*;

#[test]
fn xps_text_signature_precedes_the_generic_txt_route() {
    let casa = b"Cycle 1:1:C 1s\n\tCharacteristic Energy eV\t1486.69\nName\t\tC 1s\nPosition\t\t284.8\nFWHM\t\t1.2\nArea\t\t10\nLineshape\t\tGL(30)\nK.E.\tCounts\tC 1s\tBackground\tEnvelope\t\tB.E.\tCPS\tC 1s\tBackground CPS\tEnvelope CPS\n";
    let mut header = [0_u8; OPEN_HEADER_BYTES];
    header[..casa.len()].copy_from_slice(casa);
    let kind = classify_open_path_with_header(
        Path::new("casa.txt"),
        OpenPathEntryType::RegularFile,
        || Ok((header, casa.len())),
    )
    .unwrap();
    assert_eq!(kind, RecentOpenKind::DataFile);

    let generic = b"energy,intensity\n1,2\n";
    let mut header = [0_u8; OPEN_HEADER_BYTES];
    header[..generic.len()].copy_from_slice(generic);
    let kind = classify_open_path_with_header(
        Path::new("generic.txt"),
        OpenPathEntryType::RegularFile,
        || Ok((header, generic.len())),
    )
    .unwrap();
    assert_eq!(kind, RecentOpenKind::DelimitedTable);
}
