use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChecksumError {
    #[error("line must contain a trailing checksum field separated by comma")]
    MissingChecksum,
    #[error("checksum field contains non-hex characters")]
    InvalidHex,
    #[error("checksum mismatch")]
    Mismatch,
}

pub fn calc_bcc(input: &str) -> String {
    let bcc = input.as_bytes().iter().fold(0u8, |acc, byte| acc ^ byte);
    format!("{bcc:02X}")
}

pub fn calc_crc16_ccitt_false(input: &str) -> String {
    let mut crc: u16 = 0xFFFF;

    for byte in input.as_bytes() {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }

    format!("{crc:04X}")
}

pub fn verify_bcc(line: &str) -> bool {
    verify_checksum(line, calc_bcc).is_ok()
}

pub fn verify_crc16(line: &str) -> bool {
    verify_checksum(line, calc_crc16_ccitt_false).is_ok()
}

fn verify_checksum<F>(line: &str, calculator: F) -> Result<(), ChecksumError>
where
    F: Fn(&str) -> String,
{
    let (payload, claimed) = split_payload_and_checksum(line)?;

    if claimed.is_empty() || !claimed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ChecksumError::InvalidHex);
    }

    let expected = calculator(payload);
    if expected.eq_ignore_ascii_case(claimed) {
        Ok(())
    } else {
        Err(ChecksumError::Mismatch)
    }
}

fn split_payload_and_checksum(line: &str) -> Result<(&str, &str), ChecksumError> {
    let idx = line.rfind(',').ok_or(ChecksumError::MissingChecksum)?;
    if idx + 1 >= line.len() {
        return Err(ChecksumError::MissingChecksum);
    }

    let payload = &line[..=idx];
    let checksum = line[idx + 1..].trim();
    Ok((payload, checksum))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_range_includes_last_comma() {
        assert_eq!(calc_bcc("A,B,"), "03");
        assert_eq!(calc_bcc("A,B"), "2F");

        assert_eq!(calc_crc16_ccitt_false("A,B,"), "E10F");
        assert_eq!(calc_crc16_ccitt_false("A,B"), "CD0C");
    }

    #[test]
    fn verify_crc16_and_bcc() {
        assert!(verify_crc16(
            "8981000000000000000,2003,202602281300,1,GET_STATUS,,2CB7"
        ));
        assert!(verify_bcc("8981123456789012345,2001,OK,,3F"));

        assert!(!verify_crc16(
            "8981000000000000000,2003,202602281300,1,GET_STATUS,,2CB8"
        ));
        assert!(!verify_bcc("8981123456789012345,2001,OK,,40"));
    }
}
