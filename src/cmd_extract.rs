use crate::{
  bin_format::{BssType, Ent, Info},
  mani_json::{ManiFile, ManiJson},
};
use anyhow::{Result, anyhow};
use binrw::BinRead;
use md5::Digest;
use parseq::ParallelIterator;
use std::{
  fs::{File, create_dir_all},
  io::{BufWriter, Read, Seek, SeekFrom, Write},
  path::PathBuf,
};

pub fn extract(in_file: PathBuf, out_dir: PathBuf, list_only: bool) -> Result<()> {
  let mut file = File::open_buffered(in_file)?;
  let hdr = Info::read(&mut file)?;
  let bss = BssType::from_info(&hdr)?.read(&mut file, hdr.blk_size)?;

  println!(
    "PSAR version {}.{}, {} files",
    hdr.v_maj,
    hdr.v_min,
    hdr.ents.len() - 1
  );
  println!("- compression: {}", hdr.compr.name());
  println!("- blocks: size {} x count {}", hdr.blk_size, bss.len());
  println!(
    "- flags: ignorecase {}, absolute: {}",
    hdr.igncase, hdr.abspath
  );

  let manifest = {
    let &Ent {
      name_md5: _,
      blk_idx,
      unc_len,
      blk_off,
    } = &hdr.ents[0];
    let bl = blk_idx as usize;
    let br = bl + unc_len.div_ceil(hdr.blk_size as u64) as usize;
    let mut dat = Vec::with_capacity(unc_len.try_into()?);
    file.seek(SeekFrom::Start(blk_off))?;
    for &bs in &bss[bl..br] {
      let mut blk = vec![0u8; bs as usize];
      file.read_exact(&mut blk)?;
      dat.extend(hdr.compr.dec(&blk, hdr.blk_size as usize).unwrap_or(blk));
    }
    String::from_utf8(dat)?
  };

  let namelist = {
    let mut ss = Vec::with_capacity(hdr.ents.len());
    ss.push("__manifest.txt");
    ss.extend(manifest.lines().map(|s| s.strip_prefix('/').unwrap_or(s)));
    if ss.len() < hdr.ents.len() {
      return Err(anyhow!("not enough lines in manifest file"));
    }
    ss
  };

  if list_only {
    for (i, (e, name)) in hdr.ents.iter().zip(namelist).enumerate() {
      println!(
        "[{i:5}] name_md5={:x} length={:9} : {name}",
        Digest(e.name_md5.0),
        e.unc_len
      );
    }
    return Ok(());
  }

  let (block_infos, empty_files) = calc_blocks_info(bss, hdr.ents, hdr.blk_size)?;

  for i in empty_files {
    println!("empty file {}", namelist[i]);
    let p = out_dir.join(namelist[i]);
    if let Some(par) = p.parent() {
      create_dir_all(par)?;
    }
    File::create(p)?;
  }

  let mut err = None;
  let comp = hdr.compr;
  let mut filelist = Vec::<Option<BufWriter<File>>>::with_capacity(namelist.len());

  for _ in 0..namelist.len() {
    filelist.push(None);
  }

  for (dat, wf, cf) in block_infos
    .into_iter()
    .map_while(|bi| {
      match try {
        file.seek(SeekFrom::Start(bi.off))?;
        let mut blk = vec![0u8; bi.len as usize];
        file.read_exact(&mut blk)?;
        (blk, bi)
      } {
        Ok(ret) => Some(ret),
        Err(e) => {
          err = Some(e);
          None
        }
      }
    })
    .map_parallel(move |(blk, bi)| {
      let dat = comp.dec(&blk, bi.unc_len as usize).unwrap_or(blk);
      (dat, bi.f_write, bi.f_close)
    })
  {
    for i in wf {
      let f = &mut filelist[i];
      match f {
        Some(w) => w.write_all(&dat)?,
        None => {
          let p = out_dir.join(namelist[i]);
          println!("{} <- {}", p.display(), namelist[i]);
          if let Some(par) = p.parent() {
            create_dir_all(par)?;
          }
          let mut w = File::create_buffered(p)?;
          w.write_all(&dat)?;
          *f = Some(w);
        }
      }
    }
    for i in cf {
      if let Some(w) = &mut filelist[i] {
        w.flush()?;
        filelist[i] = None;
      }
    }
  }
  for mut f in filelist.into_iter().flatten() {
    f.flush()?;
  }

  File::create(out_dir.join("__manifest.json"))?.write_all(&serde_json::to_vec_pretty(
    &ManiJson {
      ver_maj: hdr.v_maj, //
      ver_min: hdr.v_min,
      compression: hdr.compr.name(),
      compr_level: 16,
      force_comp: Some(false),
      block_size: Some(hdr.blk_size),
      ignorecase: Some(hdr.igncase),
      absolute: Some(hdr.abspath),
      dedup: Some(true),
      files: namelist[1..]
        .iter()
        .map(|path| ManiFile {
          path, //
          name: None,
          compr_level: None,
          force_comp: None,
        })
        .collect(),
    },
  )?)?;

  Ok(())
}

#[derive(Clone)]
struct BlkInfo {
  off: u64,
  len: u32,
  unc_len: u32,
  f_write: Vec<usize>,
  f_close: Vec<usize>,
}

fn calc_blocks_info(
  bss: Vec<u32>,
  ents: Vec<Ent>,
  blk_size: u32,
) -> Result<(Vec<BlkInfo>, Vec<usize>)> {
  let mut infos = Vec::with_capacity(bss.len());

  bss
    .into_iter()
    .map(|len| BlkInfo {
      off: 0,
      len,
      unc_len: 0,
      f_write: vec![],
      f_close: vec![],
    })
    .collect_into(&mut infos);

  let mut empty = vec![];

  for (
    i,
    Ent {
      name_md5: _,
      blk_idx,
      unc_len,
      blk_off,
    },
  ) in ents.into_iter().enumerate()
  {
    let bl = blk_idx as usize;
    let br = (bl as u64 + unc_len.div_ceil(blk_size as u64).max(1)).try_into()?;
    let mut fp = blk_off;
    let mut fl = unc_len;

    for (
      j,
      BlkInfo {
        off,
        len,
        unc_len: bul,
        f_write,
        f_close: _,
      },
    ) in //
      infos
        .get_mut(bl..br)
        .ok_or_else(|| anyhow!("block index oob for file #{i}"))?
        .iter_mut()
        .enumerate()
    {
      if *off == 0 {
        *off = fp;
      } else if *off != fp {
        return Err(anyhow!("different offsets for block #{j}"));
      }
      fp += *len as u64;

      let ebl = fl.min(blk_size as u64) as u32;
      if *bul == 0 {
        *bul = ebl;
      } else if *bul != ebl {
        return Err(anyhow!("different uncompressed length for block #{j}"));
      }
      fl -= ebl as u64;

      if unc_len > 0 {
        f_write.push(i);
      }
    }
    if unc_len > 0 {
      infos[br - 1].f_close.push(i);
    } else {
      empty.push(i);
    }
  }

  for (i, bi) in infos.iter().enumerate() {
    if bi.off == 0 {
      return Err(anyhow!("block #{i} doesn't have an offset"));
    }
  }

  Ok((infos, empty))
}
