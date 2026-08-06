//! Stock zlib 1.2.3, matching the zlib embedded in orbis-psarc.
//! `compress2()` uses DEFLATED, windowBits=15, memLevel=8 and default strategy.

use anyhow::{Result, anyhow};
use std::ffi::{c_int, c_ulong};

const Z_OK: c_int = 0;

#[link(name = "zlib123", kind = "static")]
unsafe extern "C" {
  fn compressBound(source_len: c_ulong) -> c_ulong;
  fn compress2(dest: *mut u8, dest_len: *mut c_ulong, source: *const u8, source_len: c_ulong, level: c_int) -> c_int;
  fn uncompress(dest: *mut u8, dest_len: *mut c_ulong, source: *const u8, source_len: c_ulong) -> c_int;
}

pub fn compress(input: &[u8], level: u32) -> Result<Vec<u8>> {
  if !(1..=9).contains(&level) {
    return Err(anyhow!("zlib level {level} must be in 1..=9"));
  }
  let input_len: c_ulong = input.len().try_into()?;
  let mut output_len = unsafe { compressBound(input_len) };
  let mut output = vec![0; output_len as usize];
  let result = unsafe { compress2(output.as_mut_ptr(), &mut output_len, input.as_ptr(), input_len, level as c_int) };
  if result != Z_OK {
    return Err(anyhow!("zlib 1.2.3 compress2 failed with code {result}"));
  }
  output.truncate(output_len as usize);
  Ok(output)
}

pub fn decompress(input: &[u8], expected_len: usize) -> Option<Vec<u8>> {
  let input_len: c_ulong = input.len().try_into().ok()?;
  let mut output_len: c_ulong = expected_len.try_into().ok()?;
  let mut output = vec![0; expected_len];
  let result = unsafe { uncompress(output.as_mut_ptr(), &mut output_len, input.as_ptr(), input_len) };
  if result != Z_OK { return None; }
  output.truncate(output_len as usize);
  Some(output)
}
