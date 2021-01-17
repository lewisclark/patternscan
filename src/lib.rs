//! Searches for a contiguous array of bytes determined by a given pattern. The pattern can include
//! supported wildcard characters, as seen below.
//!
//! ## Wildcards
//! * `?` match any byte
//!
//! ## Example Patterns
//! * `fe 00 68 98` - matches only `fe 00 68 98`
//! * `8d 11 ? ? 8f` - could match `8d 11 9e ef 8f` or `8d 11 0 0 8f` for example
//!
//! ## Example Usage
//! The [`scan`] function is used to scan for a pattern within the output of a [`Read`]. Using a
//! [`Cursor`](std::io::Cursor) to scan within a byte array in memory could look as follows:
//!
//! ```rust
//! use patternscan::scan;
//! use std::io::Cursor;
//!
//! let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
//! let pattern = "20 30 40";
//! let locs = scan(Cursor::new(bytes), &pattern).unwrap(); // Will equal vec![1], the index of
//!                                                         // the pattern
//! ```
//!
//! Any struct implementing [`Read`] can be passed as the reader which should be scanned for
//! ocurrences of a pattern, so one could scan for a byte sequence within an executable as follows:
//!
//! ```ignore
//! use patternscan::scan;
//! use std::fs::File;
//!
//! let reader = File::open("somebinary.exe").unwrap();
//! let instruction = "A3 ? ? ? ?";
//! let locs = scan(reader, &instruction).unwrap();
//! ```
//!
//! For more example uses of this module, see the
//! [tests](https://github.com/lewisclark/patternscan/blob/master/src/lib.rs#L128)
use std::fmt::{self, Display};
use std::io::Read;
use std::str::FromStr;

/// Size of chunks to be read from `reader` when looking for patterns.
///
/// In [`Matches`] (which in turn is used in [`scan`] and [`scan_first_match`]), bytes are read
/// from the provided [`Read`] type into a fixed-size internal buffer. The length of this buffer is
/// given by `CHUNK_SIZE`.
pub const CHUNK_SIZE: usize = 0x800;

/// Scan for any instances of `pattern` in the bytes read by `reader`.
///
/// Returns a [`Result`] containing a vector of indices of the start of each match within the
/// bytes. If no matches are found, this vector will be empty. Returns an [`Error`] if an error was
/// encountered while scanning, which could occur if the pattern is invalid (i.e: contains
/// something other than 8-bit hex values and wildcards), or if the reader encounters an error.
pub fn scan(reader: impl Read, pattern: &str) -> Result<Vec<usize>, Error> {
    let matches = Matches::from_pattern_str(reader, pattern)?;
    matches.collect()
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
pub fn scan_first_match(reader: impl Read, pattern: &str) -> Result<Option<usize>, Error> {
    let pattern = Pattern::from_str(pattern)?;
    let mut matches = Matches::from_pattern(reader, pattern)?;
    matches.next().transpose()
}

/// Determine whether a byte slice matches a pattern.
pub fn pattern_matches(bytes: &[u8], pattern: &Pattern) -> bool {
    if bytes.len() < pattern.len() {
        false
    } else {
        pattern == bytes
    }
}

/// Represents an error which occurred while scanning for a pattern.
#[derive(Debug)]
pub struct Error {
    /// String detailing the error
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

/// Represents a single byte in a search pattern.
#[derive(PartialEq, Eq)]
pub enum PatternByte {
    Byte(u8),
    Any,
}

impl FromStr for PatternByte {
    type Err = Error;

    /// Create an instance of [`PatternByte`] from a string.
    ///
    /// This string should either be a hexadecimal byte, or a "?". Will return an error if the
    /// string is not a "?", or it cannot be converted into an 8-bit integer when interpreted as
    /// hexadecimal.
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

/// Represents a pattern to search for in a byte string.
#[derive(PartialEq, Eq)]
pub struct Pattern {
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

/// Iterator over locations of matches for a pattern found within a byte string.
///
/// This struct implements the actual logic for pattern matching, and is used by the [`scan`] and
/// [`scan_first_match`] functions to locate matches. The values returned by the iterator are
/// indices of locations of the pattern matches within the byte string produced by `reader`.
///
/// The byte string which the pattern should be searched against is read from `reader` in
/// [`CHUNK_SIZE`] chunks, when required by the iterator.
///
/// ## Example Usage
/// ```rust
/// use patternscan;
/// use std::io::Cursor;
///
/// let bytes = [0x10, 0x20, 0x30, 0x40];
/// let reader = Cursor::new(bytes);
/// let pattern = patternscan::Matches::from_pattern_str(reader, "20 30").unwrap();
/// let match_indices: Result<Vec<usize>, _> = pattern.collect();
/// let match_indices = match_indices.unwrap();
/// ```
pub struct Matches<R: Read> {
    /// Reader from which the byte string to search will be read.
    pub reader: R,
    /// Pattern to search for in the byte string.
    pub pattern: Pattern,

    // Internal state, would be nice to reduce this somehow
    bytes_buf: [u8; CHUNK_SIZE],
    last_bytes_read: usize,
    abs_position: usize,
    rel_position: usize,
}

impl<R: Read> Matches<R> {
    /// Create a new instance of [`Matches`] from an instance of [`Pattern`].
    ///
    /// `reader` should be some [`Read`] type which will produce a byte string to search. `pattern`
    /// should be a [`Pattern`] to search for.
    pub fn from_pattern(mut reader: R, pattern: Pattern) -> Result<Self, Error> {
        // Constraint imposed due to the method used to detect matches over chunk boundaries. We
        // might want to increase the chunk size to account for this?
        if 2 * pattern.len() > CHUNK_SIZE {
            return Err(Error::new(format!(
                "Pattern too long: It can be at most {} bytes",
                CHUNK_SIZE / 2
            )));
        }

        // Perform initial read into the bytes buffer on creation
        // Might be more idiomatic to only perform a read once we're stepping through the iterator,
        // I'm not sure, but this ensures that the state of the struct when an instance is created
        // is reasonable.
        let mut bytes_buf = [0; CHUNK_SIZE];
        let bytes_read = reader
            .read(&mut bytes_buf)
            .map_err(|e| Error::new(format!("Failed to read from reader: {}", e)))?;

        Ok(Self {
            reader,
            pattern,
            bytes_buf,
            last_bytes_read: bytes_read,
            abs_position: 0,
            rel_position: 0,
        })
    }

    /// Create a new instance of [`Matches`] from a string pattern.
    pub fn from_pattern_str(reader: R, pattern: &str) -> Result<Self, Error> {
        let pattern = Pattern::from_str(pattern)?;
        Self::from_pattern(reader, pattern)
    }
}

impl<R: Read> Iterator for Matches<R> {
    type Item = Result<usize, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.rel_position == CHUNK_SIZE - self.pattern.len() {
                // This block is what allows us to detect matches over chunk boundaries.
                // When we're close enough to a boundary that a pattern match could overrun, we
                // copy the final bytes in the buffer to the start of the buffer, then read into
                // the rest of the buffer.
                let len = self.pattern.len();

                let boundary_bytes = &self.bytes_buf[CHUNK_SIZE - len..].to_owned();
                self.bytes_buf[..len].copy_from_slice(&boundary_bytes);

                self.last_bytes_read = match self.reader.read(&mut self.bytes_buf[len..]) {
                    Ok(b) => b,
                    Err(e) => return Some(Err(Error::new(format!("Failed to read bytes: {}", e)))),
                };

                self.rel_position = 0;
            }

            if self.rel_position == self.last_bytes_read + self.pattern.len() {
                break;
            }

            for i in self.rel_position..self.last_bytes_read + self.pattern.len() {
                if i == CHUNK_SIZE - self.pattern.len() {
                    break;
                }

                self.abs_position += 1;
                self.rel_position += 1;
                if pattern_matches(&self.bytes_buf[i..], &self.pattern) {
                    return Some(Ok(self.abs_position - 1));
                }
            }

            if self.last_bytes_read == 0 {
                break;
            }
        }

        None
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

    #[test]
    fn scan_multiple_instances_of_pattern() {
        let bytes = [0x10, 0x20, 0x30, 0x10, 0x20, 0x30];
        let pattern = "10 20 30";

        assert_eq!(
            crate::scan(Cursor::new(bytes), &pattern).unwrap(),
            vec![0, 3]
        );
    }

    #[test]
    fn scan_multiple_instances_q() {
        let bytes = [0x10, 0x20, 0x30, 0x10, 0x40, 0x30];
        let pattern = "10 ? 30";

        assert_eq!(
            crate::scan(Cursor::new(bytes), &pattern).unwrap(),
            vec![0, 3]
        );
    }

    #[test]
    fn scan_rejects_invalid_pattern() {
        let bytes = [0x10, 0x20, 0x30];
        let pattern = "10 fff 20";

        assert!(crate::scan(Cursor::new(bytes), &pattern).is_err());
    }

    #[test]
    fn scan_first_match_simple_start() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "10 20 30";

        assert_eq!(
            crate::scan_first_match(Cursor::new(bytes), &pattern)
                .unwrap()
                .unwrap(),
            0
        );
    }

    #[test]
    fn scan_first_match_simple_middle() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "20 30 40";

        assert_eq!(
            crate::scan_first_match(Cursor::new(bytes), &pattern)
                .unwrap()
                .unwrap(),
            1
        );
    }

    #[test]
    fn scan_first_match_no_match() {
        let bytes = [0x10, 0x20, 0x30, 0x40, 0x50];
        let pattern = "10 11 12";

        assert!(crate::scan_first_match(Cursor::new(bytes), &pattern)
            .unwrap()
            .is_none());
    }

    #[test]
    fn find_across_chunk_boundary() {
        let mut bytes = vec![0; super::CHUNK_SIZE - 2];
        bytes.push(0xaa);
        bytes.push(0xbb);
        bytes.push(0xcc);
        bytes.push(0xdd);
        let pattern = "aa bb cc dd";

        assert!(crate::scan_first_match(Cursor::new(bytes), &pattern)
            .unwrap()
            .is_some())
    }
}
