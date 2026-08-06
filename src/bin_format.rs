use anyhow::{Result, anyhow};
use binrw::binrw;
use std::{
  fmt::Display,
  io::{Read, Write},
};

#[binrw]
#[brw(big, magic = b"PSAR")]
pub struct Info {
  pub v_maj: u16,
  pub v_min: u16,
  pub compr: Comp,
  pub info_len: u32,
  #[br(temp, assert(ent_size == 30, "ent_size ({ent_size}) must be 30"))]
  #[bw(calc(30))]
  pub ent_size: u32,
  #[br(temp, assert(ents_cnt >= 1, "must be at least one file (manifest)"))]
  #[bw(assert(!ents.is_empty()), try_calc(ents.len().try_into()))]
  pub ents_cnt: u32,
  #[brw(assert(blk_size.is_power_of_two(), "blk_size ({blk_size}) must be a power of two"))]
  pub blk_size: u32,
  #[br(temp)]
  #[bw(calc(*flags))]
  pub fl: u32,
  #[br(calc(fl))]
  #[bw(ignore)]
  pub flags: u32,
  #[br(calc(fl & 0b01 != 0))]
  #[bw(ignore)]
  pub igncase: bool,
  #[br(calc(fl & 0b10 != 0))]
  #[bw(ignore)]
  pub abspath: bool,
  #[br(count = ents_cnt)]
  pub ents: Vec<Ent>,
}

#[binrw]
#[derive(Clone, Copy)]
pub enum Comp {
  #[brw(magic = b"zlib")]
  Zlib,
  #[brw(magic = b"lzma")]
  Lzma,
}

impl Comp {
  pub const fn name(&self) -> &'static str {
    match self {
      Comp::Zlib => "zlib",
      Comp::Lzma => "lzma",
    }
  }

  pub fn dec(&self, i: &[u8], l: usize) -> Option<Vec<u8>> {
    match self {
      Comp::Zlib => crate::zlib123::decompress(i, l),
      Comp::Lzma => todo!(),
    }
  }

  pub fn com(&self, i: &[u8], l: u32) -> Result<Vec<u8>> {
    match self {
      Comp::Zlib => crate::zlib123::compress(i, l),
      Comp::Lzma => todo!(),
    }
  }
}

pub enum BssType {
  U8(u32),
  U16(u32),
  U24(u32),
  U32(u32),
}

impl BssType {
  pub const fn len(&self) -> Option<u32> {
    match self {
      BssType::U8(n) => Some(*n),
      BssType::U16(n) => n.checked_mul(2),
      BssType::U24(n) => n.checked_mul(3),
      BssType::U32(n) => n.checked_mul(4),
    }
  }

  pub fn from_info(h: &Info) -> Result<Self> {
    let hto_len = h.ents.len() as u64 * 30 + 32;
    let bss_len = h
      .info_len
      .checked_sub(hto_len.try_into()?)
      .ok_or_else(|| anyhow!("info_len < hdr + toc"))?;
    let (int, chk, val) = if h.blk_size <= 0x100 {
      (1, 0, Self::U8(bss_len))
    } else if h.blk_size <= 0x10000 {
      (2, bss_len % 2, Self::U16(bss_len / 2))
    } else if h.blk_size <= 0x1000000 {
      (3, bss_len % 3, Self::U24(bss_len / 3))
    } else {
      (4, bss_len % 4, Self::U32(bss_len / 4))
    };
    if chk != 0 {
      Err(anyhow!(
        "space for bss ({bss_len}) is not divisible by {int} (blk_size {})",
        h.blk_size
      ))
    } else {
      Ok(val)
    }
  }

  pub fn from_blksz_cnt(blk_size: u32, blk_cnt: u32) -> Self {
    if blk_size <= 0x100 {
      Self::U8(blk_cnt)
    } else if blk_size <= 0x10000 {
      Self::U16(blk_cnt)
    } else if blk_size <= 0x1000000 {
      Self::U24(blk_cnt)
    } else {
      Self::U32(blk_cnt)
    }
  }

  pub fn read_raw<R: Read>(&self, r: &mut R) -> Result<Vec<u32>> {
    let v = match *self {
      Self::U8(n) => {
        let mut b = vec![0u8; n as usize];
        let mut v = vec![0u32; n as usize];
        r.read_exact(&mut b)?;
        for (rb, vb) in b.into_iter().zip(&mut v) {
          *vb = rb as u32;
        }
        v
      }
      Self::U16(n) => {
        let mut v = vec![0u32; n as usize];
        for vb in &mut v {
          *vb = u16::from_be_bytes(r.read_array()?) as u32;
        }
        v
      }
      Self::U24(n) => {
        let mut v = vec![0u32; n as usize];
        for vb in &mut v {
          let [x, y, z] = r.read_array()?;
          *vb = u32::from_be_bytes([0, x, y, z]);
        }
        v
      }
      Self::U32(n) => {
        let mut v = vec![0u32; n as usize];
        for vb in &mut v {
          *vb = u32::from_be_bytes(r.read_array()?);
        }
        v
      }
    };
    Ok(v)
  }

  pub fn write<W: Write>(&self, w: &mut W, sizes: &[u32]) -> Result<()> {
    match self {
      BssType::U8(_) => {
        for &v in sizes {
          w.write_all(&v.to_be_bytes()[3..4])?;
        }
      }
      BssType::U16(_) => {
        for &v in sizes {
          w.write_all(&v.to_be_bytes()[2..4])?;
        }
      }
      BssType::U24(_) => {
        for &v in sizes {
          w.write_all(&v.to_be_bytes()[1..4])?;
        }
      }
      BssType::U32(_) => {
        for &v in sizes {
          w.write_all(&v.to_be_bytes())?;
        }
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::BssType;

  #[test]
  fn zsize_full_raw_block_is_encoded_as_zero() {
    let mut bytes = Vec::new();
    BssType::U16(1).write(&mut bytes, &[65536]).unwrap();
    assert_eq!(bytes, [0, 0]);
  }
}

#[binrw]
pub struct Ent {
  pub name_md5: NameMd5,
  pub blk_idx: u32,
  #[br(temp)]
  #[bw(try_calc((unc_len >> 32).try_into()))]
  pub fl_hi: u8,
  #[br(temp)]
  #[bw(calc(*unc_len as _))]
  pub fl_lo: u32,
  #[br(temp)]
  #[bw(try_calc((blk_off >> 32).try_into()))]
  pub bo_hi: u8,
  #[br(temp)]
  #[bw(calc(*blk_off as _))]
  pub bo_lo: u32,
  #[br(calc(((fl_hi as u64) << 32) + fl_lo as u64))]
  #[bw(ignore)]
  pub unc_len: u64,
  #[br(calc(((bo_hi as u64) << 32) + bo_lo as u64))]
  #[bw(ignore)]
  pub blk_off: u64,
}

#[binrw]
pub struct NameMd5(pub [u8; 16]);

impl Display for NameMd5 {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    for c in self.0 {
      f.write_fmt(format_args!("{c:02x}"))?;
    }
    Ok(())
  }
}
