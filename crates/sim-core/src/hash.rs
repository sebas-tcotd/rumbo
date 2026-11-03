//! Hash FNV-1a de 64 bits: simple, sin dependencias y estable entre plataformas.
//! No es criptográfico; sirve para detectar divergencias, no para seguridad.

#[derive(Clone, Copy)]
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    /// Escribe los bits exactos del flotante (little-endian), no su representación textual.
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.u32(v.to_bits())
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32).bytes(s.as_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }

    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

pub fn to_hex(v: u64) -> String {
    format!("{v:016x}")
}
