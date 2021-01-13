use std::fmt::{self, Display};
use std::io::Read;
use std::str::FromStr;

const CHUNK_SIZE: usize = 0x800;

pub fn scan(mut reader: impl Read, pattern: &str) -> Result<Vec<usize>, Error> {
    let pattern = Pattern::from_str(pattern)?;
    let mut matches = Vec::new();

    let mut bytes = [0; CHUNK_SIZE];
    loop {
        let bytes_written = reader
            .read(&mut bytes)
            .map_err(|e| Error::new(format!("Failed to read from reader: {}", e)))?;

        if bytes_written == 0 {
            break;
        }

        for i in 0..bytes_written {
            if pattern_matches(&bytes[i..], &pattern) {
                matches.push(i);
            }
        }
    }

    Ok(matches)
}

/// Scan for the first instance of `pattern` in the bytes read by `reader`.
///
/// This function should be used instead of [`scan`] if you just want to test whether a byte string
/// contains a given pattern, or just find the first instance, as it returns as soon as the first
/// match is found, and will therefore be more efficient for these purposes in long strings where
/// the pattern occurs early.
///
/// Returns a [`Result`] containing an [`Option`]. This Option will contain `Some(index)` if the
/// pattern was found, where `index` is the index of the first match, and `None` if the pattern
/// was not found. Returns an [`Error`] if an error was encountered while scanning, which could
/// occur if the pattern is invalid (i.e: contains something other than 8-bit hex values and
/// wildcards), or if the reader encounters an error.
pub fn scan_first_match(mut reader: impl Read, pattern: &str) -> Result<Option<usize>, Error> {
    let pattern = Pattern::from_str(pattern)?;

    let mut bytes = [0; CHUNK_SIZE];
    loop {
        let bytes_written = reader
            .read(&mut bytes)
            .map_err(|e| Error::new(format!("Failed to read from reader: {}", e)))?;

        if bytes_written == 0 {
            break;
        }

        for i in 0..bytes_written {
            if pattern_matches(&bytes[i..], &pattern) {
                return Ok(Some(i));
            }
        }
    }

    Ok(None)
}

fn pattern_matches(bytes: &[u8], pattern: &Pattern) -> bool {
    if bytes.len() < pattern.len() {
        false
    } else {
        pattern == bytes
    }
}

// Error
#[derive(Debug)]
pub struct Error {
    e: String,
}

impl Error {
    pub fn new(e: String) -> Self {
        Self { e }
    }
}

impl Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Pattern scan error: {}", self.e)
    }
}

impl std::error::Error for Error {}

// PatternByte
#[derive(PartialEq, Eq)]
enum PatternByte {
    Byte(u8),
    Any,
}

impl FromStr for PatternByte {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        if s == "?" {
            Ok(Self::Any)
        } else {
            let n = match u8::from_str_radix(s, 16) {
                Ok(n) => Ok(n),
                Err(e) => Err(Error::new(format!("from_str_radix failed: {}", e))),
            }?;

            Ok(Self::Byte(n))
        }
    }
}

impl PartialEq<u8> for PatternByte {
    fn eq(&self, other: &u8) -> bool {
        match self {
            PatternByte::Any => true,
            PatternByte::Byte(b) => b == other,
        }
    }
}

// Pattern
#[derive(PartialEq, Eq)]
struct Pattern {
    bytes: Vec<PatternByte>,
}

impl Pattern {
    fn new(bytes: Vec<PatternByte>) -> Self {
        Self { bytes }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }
}

impl FromStr for Pattern {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let mut bytes = Vec::new();

        for segment in s.split_ascii_whitespace() {
            bytes.push(PatternByte::from_str(segment)?);
        }

        Ok(Self::new(bytes))
    }
}

impl PartialEq<[u8]> for Pattern {
    fn eq(&self, other: &[u8]) -> bool {
        Iterator::zip(self.bytes.iter(), other.iter()).all(|(pb, b)| pb == b)
    }
}

// Tests
#[cfg(test)]
mod tests {
    use std::io::Cursor;

    #[test]
    fn simple_scan_start() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "10 20 30";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![0]);
    }

    #[test]
    fn simple_scan_middle() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "20 30 40";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![1]);
    }

    #[test]
    fn scan_bad_exceeds() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "40 50 60";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![]);
    }

    #[test]
    fn scan_exists() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "fe 7c 88 fd 90 0";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![1]);
    }

    #[test]
    fn scan_exists_multiple_q() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "fe ? ? ? 90";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![1]);
    }

    #[test]
    fn scan_exists_multiple_q_starts() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "? ? ? ? fd";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![0]);
    }

    #[test]
    fn scan_nexists_1() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "78 90 cc dd fe";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![]);
    }

    #[test]
    fn scan_nexists_2() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "fe 7c 88 fd 90 1";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![]);
    }

    #[test]
    fn scan_pattern_larger_than_bytes() {
        let bytes = [0xff, 0xfe, 0x7c, 0x88, 0xfd, 0x90, 0x00];
        let pattern = "fe 7c 88 fd 90 0 1";

        assert_eq!(crate::scan(Cursor::new(bytes), &pattern).unwrap(), vec![]);
    }
}
