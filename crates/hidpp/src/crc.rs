//! Sector checksum.
//!
//! Port of `hidpp_crc_ccitt()` from libratbag `src/hidpp-generic.c` (seed 0xFFFF,
//! polynomial 0x1021, no reflection, no final xor; Solaar's table-driven
//! `common.crc16()` is the same function). The CRC covers every byte of the sector
//! except the last two, and is stored big-endian in those last two bytes.

pub fn crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let temp = (crc >> 8) ^ u16::from(byte);
        crc <<= 8;
        let mut quick = temp ^ (temp >> 4);
        crc ^= quick;
        quick <<= 5;
        crc ^= quick;
        quick <<= 7;
        crc ^= quick;
    }
    crc
}

/// CRC stored in the last two bytes of a sector.
pub fn stored_crc(sector: &[u8]) -> u16 {
    let n = sector.len();
    u16::from_be_bytes([sector[n - 2], sector[n - 1]])
}

/// CRC computed over everything but the last two bytes.
pub fn computed_crc(sector: &[u8]) -> u16 {
    crc_ccitt(&sector[..sector.len() - 2])
}

pub fn is_valid(sector: &[u8]) -> bool {
    sector.len() > 2 && stored_crc(sector) == computed_crc(sector)
}

/// Recompute and store the CRC in place.
pub fn seal(sector: &mut [u8]) {
    let crc = computed_crc(sector);
    let n = sector.len();
    sector[n - 2..].copy_from_slice(&crc.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_value() {
        // CRC-16/CCITT-FALSE check value.
        assert_eq!(crc_ccitt(b"123456789"), 0x29B1);
        assert_eq!(crc_ccitt(&[]), 0xFFFF);
    }

    #[test]
    fn seal_then_valid() {
        let mut s = vec![0xAB; 256];
        assert!(!is_valid(&s));
        seal(&mut s);
        assert!(is_valid(&s));
        s[10] ^= 1;
        assert!(!is_valid(&s));
    }
}
