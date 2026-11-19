use sha2::{Digest, Sha256};
use std::io::Read;

const HASH_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityError {
    InvalidExpectedSha256,
    Io(String),
    SizeOverflow,
    SizeMismatch { expected: u64, actual: u64 },
    HashMismatch { expected: String, actual: String },
}

/// Verify a downloaded artifact without reading the whole file into memory.
pub fn verify_reader<R: Read>(
    mut reader: R,
    expected_sha256: &str,
    expected_size_bytes: Option<u64>,
) -> Result<VerificationReport, IntegrityError> {
    if expected_sha256.len() != 64
        || !expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(IntegrityError::InvalidExpectedSha256);
    }

    let mut hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| IntegrityError::Io(error.to_string()))?;
        if count == 0 {
            break;
        }
        total_bytes = total_bytes
            .checked_add(count as u64)
            .ok_or(IntegrityError::SizeOverflow)?;
        hasher.update(&buffer[..count]);
    }

    if let Some(expected) = expected_size_bytes {
        if total_bytes != expected {
            return Err(IntegrityError::SizeMismatch {
                expected,
                actual: total_bytes,
            });
        }
    }

    let actual_sha256 = hex::encode(hasher.finalize());
    if actual_sha256 != expected_sha256 {
        return Err(IntegrityError::HashMismatch {
            expected: expected_sha256.to_owned(),
            actual: actual_sha256,
        });
    }

    Ok(VerificationReport {
        size_bytes: total_bytes,
        sha256: actual_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_known_sha256_and_size() {
        let report = verify_reader(
            &b"hello"[..],
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            Some(5),
        )
        .unwrap();

        assert_eq!(report.size_bytes, 5);
        assert_eq!(
            report.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn rejects_wrong_hash() {
        assert!(matches!(
            verify_reader(&b"hello"[..], &"0".repeat(64), None),
            Err(IntegrityError::HashMismatch { .. })
        ));
    }

    #[test]
    fn rejects_wrong_size() {
        assert_eq!(
            verify_reader(
                &b"hello"[..],
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
                Some(6),
            ),
            Err(IntegrityError::SizeMismatch {
                expected: 6,
                actual: 5,
            })
        );
    }
}
