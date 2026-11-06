//! Hash FNV-1a de 64 bits: simple, sin dependencias y estable entre plataformas.
//! No es criptográfico; sirve para detectar divergencias, no para seguridad.

#[derive(Clone, Copy)]
/// Platform-independent legacy FNV-1a 64-bit accumulator.
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// Start at the FNV offset basis.
    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// Append bytes in the supplied order.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        self
    }

    /// Append a little-endian 32-bit unsigned integer.
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    /// Append a little-endian 64-bit unsigned integer.
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    /// Escribe los bits exactos del flotante (little-endian), no su representación textual.
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.u32(v.to_bits())
    }

    /// Simulation legacy text protocol: four-byte length followed by UTF-8.
    pub fn str_u32_len(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32).bytes(s.as_bytes())
    }

    /// Compiler legacy text protocol: eight-byte length followed by UTF-8.
    pub fn str_u64_len(&mut self, s: &str) -> &mut Self {
        self.u64(s.len() as u64).bytes(s.as_bytes())
    }
    /// Signed integer bits, little endian.
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    /// Exact double precision bits, little endian.
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.u64(v.to_bits())
    }
    /// Return the current 64-bit hash.
    pub fn finish(&self) -> u64 {
        self.0
    }

    /// Return a zero-padded lowercase hexadecimal hash.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Format a legacy 64-bit hash as 16 lowercase hexadecimal characters.
pub fn to_hex(v: u64) -> String {
    format!("{v:016x}")
}
