pub struct Writer<'a> {
    buf: &'a mut Vec<u8>,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut Vec<u8>) -> Self {
        Self { buf }
    }

    #[inline]
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    #[inline]
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    #[inline]
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.u32(v.to_bits())
    }

    #[inline]
    pub fn varint(&mut self, mut v: u64) -> &mut Self {
        while v >= 0x80 {
            self.buf.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
        self
    }

    #[inline]
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    pub fn blob(&mut self, v: &[u8]) -> &mut Self {
        self.varint(v.len() as u64).bytes(v)
    }

    pub fn str(&mut self, v: &str) -> &mut Self {
        self.blob(v.as_bytes())
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    #[inline]
    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    #[inline]
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let s = self.data.get(self.pos..self.pos + N)?;
        self.pos += N;
        Some(s.try_into().unwrap())
    }

    #[inline]
    pub fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }

    #[inline]
    pub fn u16(&mut self) -> Option<u16> {
        self.take().map(u16::from_le_bytes)
    }

    #[inline]
    pub fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }

    #[inline]
    pub fn u64(&mut self) -> Option<u64> {
        self.take().map(u64::from_le_bytes)
    }

    #[inline]
    pub fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }

    pub fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    pub fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }

    pub fn blob(&mut self) -> Option<&'a [u8]> {
        let n = self.varint()? as usize;
        self.bytes(n)
    }

    pub fn str(&mut self) -> Option<&'a str> {
        std::str::from_utf8(self.blob()?).ok()
    }
}
