use super::{decode_wire_bytes, encode_wire_bytes, wire_data_bits, SerialParityWireMode};

#[test]
fn mac_mark_and_space_wire_bytes_round_trip_seven_bit_values() {
    let logical = [0x00, 0x41, 0x7f];
    let mark_wire = encode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &logical).unwrap();
    let space_wire = encode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &logical).unwrap();
    assert_eq!(mark_wire, [0x80, 0xc1, 0xff]);
    assert_eq!(space_wire, logical);
    assert_eq!(
        decode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &mark_wire).unwrap(),
        logical
    );
    assert_eq!(
        decode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &space_wire).unwrap(),
        logical
    );
}

#[test]
fn mac_mark_and_space_reject_eight_bit_payloads_or_bad_wire_parity() {
    assert!(encode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &[0x80]).is_err());
    assert!(encode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &[0x80]).is_err());
    assert!(decode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &[0x01]).is_err());
    assert!(decode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &[0x81]).is_err());
}

#[test]
fn emulated_parity_uses_eight_wire_data_bits() {
    assert_eq!(wire_data_bits(7, SerialParityWireMode::MacSevenBitMark), 8);
    assert_eq!(wire_data_bits(8, SerialParityWireMode::Native), 8);
}

#[cfg(unix)]
#[tokio::test]
async fn serial_io_applies_mark_parity_on_the_wire_without_a_physical_device() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_serial::SerialStream;

    let (left, right) = SerialStream::pair().unwrap();
    let mut writer = super::SerialIo::new(
        left,
        SerialParityWireMode::MacSevenBitMark,
        super::SerialRs485Mode::Disabled,
    );
    let mut raw_reader = right;
    writer.write_all(&[0x41]).await.unwrap();
    let mut wire = [0_u8; 1];
    raw_reader.read_exact(&mut wire).await.unwrap();
    assert_eq!(wire, [0xc1]);

    let (raw_writer, right) = SerialStream::pair().unwrap();
    let mut raw_writer = raw_writer;
    let mut reader = super::SerialIo::new(
        right,
        SerialParityWireMode::MacSevenBitMark,
        super::SerialRs485Mode::Disabled,
    );
    raw_writer.write_all(&[0xc1]).await.unwrap();
    let mut logical = [0_u8; 1];
    reader.read_exact(&mut logical).await.unwrap();
    assert_eq!(logical, [0x41]);
}
