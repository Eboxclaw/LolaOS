//! K0 — our own GGUF reader (`docs/ENGINE.md` §2).
//!
//! GGUF is the *interchange* format: official Liquid checkpoints drop in
//! unmodified, and the install-time repack writes the lola-native layout.
//! This module parses the container — header, metadata KV pairs, tensor
//! infos — without touching tensor data (offsets only). Pure Rust, no
//! llama.cpp code anywhere.
//!
//! Spec: https://github.com/ggml-org/ggml/blob/master/docs/gguf.md

use std::collections::BTreeMap;

/// GGUF magic bytes.
pub const GGUF_MAGIC: &[u8; 4] = b"GGUF";
/// Tensor data alignment (must be a multiple of 8 in GGUF v3).
pub const DATA_ALIGNMENT: u64 = 32;

/// Metadata value types (GGUF v3 numbering per the spec).
#[derive(Debug, Clone, PartialEq)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    String(String),
    U64(u64),
    I64(i64),
    F64(f64),
    Array(Vec<GgufValue>),
}

impl GgufValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U8(v) => Some(*v as u64),
            Self::U16(v) => Some(*v as u64),
            Self::U32(v) => Some(*v as u64),
            Self::U64(v) => Some(*v),
            _ => None,
        }
    }
}

/// One tensor's geometry (data lives in the file's data section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufTensorInfo {
    pub name: String,
    /// Dimensions, fastest-varying first, as stored by GGUF.
    pub dims: Vec<u64>,
    /// GGML type id (e.g. 0=F32, 8=Q4_0, 14=Q4_K …). Quant decode is K0+.
    pub ggml_type: u32,
    /// Byte offset of this tensor's data, relative to the data section.
    pub offset: u64,
}

/// A parsed GGUF container.
#[derive(Debug, Clone, PartialEq)]
pub struct GgufFile {
    pub version: u32,
    pub metadata: BTreeMap<String, GgufValue>,
    pub tensors: Vec<GgufTensorInfo>,
    /// Byte offset where the (aligned) tensor data section begins.
    pub data_offset: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum GgufError {
    #[error("truncated GGUF: {0}")]
    Truncated(&'static str),
    #[error("bad magic: {0:?}")]
    BadMagic([u8; 4]),
    #[error("unsupported version {0}")]
    UnsupportedVersion(u32),
    #[error("bad metadata type {0}")]
    BadMetadataType(u32),
    #[error("tensor name not utf-8")]
    BadName,
}

impl GgufFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, GgufError> {
        let mut r = Reader { bytes, pos: 0 };
        let magic = r.take4()?;
        if magic != *GGUF_MAGIC {
            return Err(GgufError::BadMagic(magic));
        }
        let version = r.u32()?;
        if !(2..=3).contains(&version) {
            return Err(GgufError::UnsupportedVersion(version));
        }
        let tensor_count = r.u64()?;
        let kv_count = r.u64()?;

        let mut metadata = BTreeMap::new();
        for _ in 0..kv_count {
            let key = r.gguf_string()?;
            let value = r.gguf_value()?;
            metadata.insert(key, value);
        }

        let mut tensors = Vec::with_capacity(tensor_count.min(4096) as usize);
        for _ in 0..tensor_count {
            let name = r.gguf_string()?;
            let n_dims = r.u32()?;
            if n_dims > 4 {
                return Err(GgufError::Truncated("n_dims > 4"));
            }
            let mut dims = Vec::with_capacity(n_dims as usize);
            for _ in 0..n_dims {
                dims.push(r.u64()?);
            }
            let ggml_type = r.u32()?;
            let offset = r.u64()?;
            tensors.push(GgufTensorInfo {
                name,
                dims,
                ggml_type,
                offset,
            });
        }

        // The data section starts at the current position, aligned up.
        let data_offset = r.pos.align_offset(DATA_ALIGNMENT as usize) as u64;
        Ok(Self {
            version,
            metadata,
            tensors,
            data_offset,
        })
    }

    /// Convenience: the architecture string (`general.architecture`).
    pub fn architecture(&self) -> Option<&str> {
        self.metadata
            .get("general.architecture")
            .and_then(GgufValue::as_str)
    }

    pub fn tensor(&self, name: &str) -> Option<&GgufTensorInfo> {
        self.tensors.iter().find(|t| t.name == name)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], GgufError> {
        if self.pos + n > self.bytes.len() {
            return Err(GgufError::Truncated("unexpected end of file"));
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take4(&mut self) -> Result<[u8; 4], GgufError> {
        let slice = self.take(4)?;
        Ok([slice[0], slice[1], slice[2], slice[3]])
    }

    fn u32(&mut self) -> Result<u32, GgufError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, GgufError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn gguf_string(&mut self) -> Result<String, GgufError> {
        let len = self.u64()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| GgufError::BadName)
    }

    fn scalar(&mut self, value_type: u32) -> Result<GgufValue, GgufError> {
        let take1 = |r: &mut Self| Ok(r.take(1)?[0]);
        Ok(match value_type {
            0 => GgufValue::U8(take1(self)?),
            1 => GgufValue::I8(take1(self)? as i8),
            2 => GgufValue::U16(u16::from_le_bytes(self.take(2)?.try_into().unwrap())),
            3 => GgufValue::I16(i16::from_le_bytes(self.take(2)?.try_into().unwrap())),
            4 => GgufValue::U32(self.u32()?),
            5 => GgufValue::I32(i32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            6 => GgufValue::F32(f32::from_le_bytes(self.take(4)?.try_into().unwrap())),
            7 => GgufValue::Bool(take1(self)? != 0),
            8 => GgufValue::String(self.gguf_string()?),
            10 => GgufValue::U64(self.u64()?),
            11 => GgufValue::I64(i64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            12 => GgufValue::F64(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            other => return Err(GgufError::BadMetadataType(other)),
        })
    }

    fn gguf_value(&mut self) -> Result<GgufValue, GgufError> {
        let value_type = self.u32()?;
        if value_type == 9 {
            // Array: element type + count.
            let elem_type = self.u32()?;
            let count = self.u64()? as usize;
            let mut items = Vec::with_capacity(count.min(1 << 20));
            for _ in 0..count {
                items.push(self.scalar(elem_type)?);
            }
            return Ok(GgufValue::Array(items));
        }
        self.scalar(value_type)
    }
}

/// Align `pos` up to `align` (power of two).
trait AlignUp {
    fn align_offset(&self, align: usize) -> usize;
}

impl AlignUp for usize {
    fn align_offset(&self, align: usize) -> usize {
        (self + align - 1) & !(align - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic GGUF file in-memory: writer mirror of the reader.
    pub(crate) fn synthetic_gguf() -> Vec<u8> {
        fn u32b(v: u32) -> Vec<u8> {
            v.to_le_bytes().to_vec()
        }
        fn u64b(v: u64) -> Vec<u8> {
            v.to_le_bytes().to_vec()
        }
        fn s(v: &str) -> Vec<u8> {
            let mut out = u64b(v.len() as u64);
            out.extend_from_slice(v.as_bytes());
            out
        }
        let mut out = Vec::new();
        out.extend_from_slice(GGUF_MAGIC);
        out.extend(u32b(3));
        out.extend(u64b(2)); // tensor count
        out.extend(u64b(5)); // kv count

        out.extend(s("general.architecture"));
        out.extend(u32b(8));
        out.extend(s("lfm3"));
        out.extend(s("lfm3.block_count"));
        out.extend(u32b(4));
        out.extend(u32b(24));
        out.extend(s("general.quantization"));
        out.extend(u32b(8));
        out.extend(s("q4_k_m"));
        out.extend(s("lfm3.conv_kernel"));
        out.extend(u32b(4));
        out.extend(u32b(7));
        out.extend(s("general.languages"));
        out.extend(u32b(9));
        out.extend(u32b(8)); // element type: string
        out.extend(u64b(2));
        out.extend(s("en"));
        out.extend(s("pt"));

        // Two tensor infos.
        let tensor = |name: &str, dims: &[u64], ggml_type: u32, offset: u64, out: &mut Vec<u8>| {
            out.extend(s(name));
            out.extend(u32b(dims.len() as u32));
            for d in dims {
                out.extend(u64b(*d));
            }
            out.extend(u32b(ggml_type));
            out.extend(u64b(offset));
        };
        tensor("blk.0.conv_wv", &[2048, 2048, 7], 0, 0, &mut out);
        tensor("blk.0.conv_wg", &[2048, 2048, 7], 0, 32, &mut out);

        // Pad to the data-section alignment, then some payload bytes.
        while out.len() % DATA_ALIGNMENT as usize != 0 {
            out.push(0);
        }
        out.extend_from_slice(&[0xAB; 64]);
        out
    }

    #[test]
    fn synthetic_file_round_trips() {
        let bytes = synthetic_gguf();
        let file = GgufFile::parse(&bytes).expect("parse");
        assert_eq!(file.version, 3);
        assert_eq!(file.architecture(), Some("lfm3"));
        assert_eq!(
            file.metadata
                .get("lfm3.block_count")
                .and_then(GgufValue::as_u64),
            Some(24)
        );
        assert_eq!(
            file.metadata.get("general.languages"),
            Some(&GgufValue::Array(vec![
                GgufValue::String("en".into()),
                GgufValue::String("pt".into()),
            ]))
        );
        assert_eq!(file.tensors.len(), 2);
        let conv = file.tensor("blk.0.conv_wv").expect("tensor present");
        assert_eq!(conv.dims, vec![2048, 2048, 7]);
        assert_eq!(conv.ggml_type, 0);
        // Data section is aligned and the payload is reachable.
        assert_eq!(file.data_offset as usize % DATA_ALIGNMENT as usize, 0);
        assert!(bytes.len() > file.data_offset as usize);
    }

    #[test]
    fn bad_magic_and_truncation_fail_closed() {
        let mut bytes = synthetic_gguf();
        bytes[0] = b'X';
        assert!(matches!(
            GgufFile::parse(&bytes),
            Err(GgufError::BadMagic(_))
        ));

        let truncated = &synthetic_gguf()[..20];
        assert!(matches!(
            GgufFile::parse(truncated),
            Err(GgufError::Truncated(_))
        ));
    }
}
