use crate::{
  bin_format::{BssType, Comp, Ent, Info, NameMd5},
  mani_json::ManiJson,
};
use anyhow::{Context, Result, anyhow};
use binrw::BinWrite;
use parseq::ParallelIterator;
use relative_path::PathExt;
use std::{
  cell::RefCell,
  collections::HashMap,
  fs::{self, File, metadata},
  hash::{DefaultHasher, Hasher},
  io::{Cursor, Read, Seek, SeekFrom, Write},
  path::{Path, PathBuf},
  str::FromStr,
};

struct PreA {
  path: PathBuf,
  arc_name: String,
  name_md5: md5::Digest,
  unc_len: u64,
  n_blk: u32,
  c_force: bool,
  c_level: i32,
  align_raw: bool,
}

pub fn create(in_json: PathBuf, out_file: PathBuf) -> Result<()> {
  let json_text = fs::read_to_string(&in_json)?;
  let json: ManiJson = serde_json::from_str(json_text.as_str())?;
  let is_orbis_ps4 = json.profile.as_deref() == Some("orbis_ps4");
  let v_maj = json.ver_maj;
  let v_min = json.ver_min;

  println!(
    "PSAR version {}.{}, {} files",
    v_maj,
    v_min,
    json.files.len()
  );

  let comp = match json.compression.as_str() {
    "zlib" => Comp::Zlib,
    "lzma" => Comp::Lzma,
    s => return Err(anyhow!("unknown compression {s}")),
  };

  let compression_enabled = json.compression_enabled.unwrap_or(true);
  let c_level = json.compression_level.unwrap_or(9);
  if !(1..=9).contains(&c_level) {
    return Err(anyhow!("compression_level {c_level} must be in 1..=9"));
  }
  let c_level = if compression_enabled { c_level as i32 } else { -1 };
  let c_force = json.force_comp.unwrap_or(false);
  println!(
    "- compression: {} level {}, force: {}",
    comp.name(),
    c_level,
    c_force
  );

  let blk_size = json.block_size.unwrap_or(65536);
  if !blk_size.is_power_of_two() {
    return Err(anyhow!("blocksize {blk_size} is not a power of two"));
  }

  println!("- blocks: size {blk_size}");

  // PS3 1.4 aligns raw files larger than the threshold (">").
  // Orbis 1.4 aligns raw files at least as large as the threshold (">=").
  let raw_align_size = json
    .file_align_size
    .unwrap_or(if is_orbis_ps4 { 2097152 } else { 65536 });
  let raw_alignment = json
    .file_alignment
    .unwrap_or(if is_orbis_ps4 { 65536 } else { 8192 }) as u64;

  if !raw_alignment.is_power_of_two() {
    return Err(anyhow!("file_alignment {raw_alignment} is not a power of two"));
  }

  let abspath = json.absolute.unwrap_or(false);
  let igncase = json.ignorecase.unwrap_or(false);
  let (manifest, pre_a) = {
    println!("- flags: ignorecase {}, absolute: {}", igncase, abspath);

    let mut pre_a = Vec::with_capacity(json.files.len() + 1);

    pre_a.push(PreA {
      path: PathBuf::from_str("manifest").unwrap(),
      arc_name: String::new(),
      name_md5: md5::Digest([0u8; 16]),
      unc_len: 0,
      c_force: if json.compress_manifest.unwrap_or(false) { c_force } else { false },
      c_level: if compression_enabled && json.compress_manifest.unwrap_or(false) { c_level } else { -1 },
      align_raw: false,
      n_blk: 0,
    });

    let in_dir = in_json.parent().unwrap_or_else(|| Path::new("."));

    for (i, mf) in json.files.iter().enumerate() {
      let mf_path = in_dir.join(&mf.path);
      let mf_name = match mf.name.as_deref() {
        Some(s) => s.to_string(),
        None => {
          let r = mf_path.relative_to(in_dir)?;
          if r.starts_with("..") {
            return Err(anyhow!(
              "{} is not a subpath of input dir {}",
              mf.path,
              in_dir.display()
            ));
          }

          let mut s = r.into_string();
          if abspath && !s.starts_with('/') {
            s.insert(0, '/');
          }
          s
        }
      };

      let name_md5 = if igncase {
        md5::compute(mf_name.to_ascii_uppercase())
      } else {
        md5::compute(&mf_name)
      };

      let unc_len = metadata(&mf_path)
        .with_context(|| anyhow!("stat {}", mf_path.display()))?
        .len();

      println!(
        "[{:5}] name_md5={name_md5:x} length={unc_len:9} : {}",
        i + 1,
        mf_name
      );

      let file_level = if mf.compressed == Some(false) {
        -1
      } else {
        mf.compression_level.unwrap_or(c_level as u32) as i32
      };

      pre_a.push(PreA {
        path: mf_path,
        arc_name: mf_name.clone(),
        name_md5,
        unc_len,
        c_force: mf.force_comp.unwrap_or(c_force),
        c_level: file_level,
        align_raw: file_level < 0
          && if is_orbis_ps4 {
            unc_len >= raw_align_size
          } else {
            unc_len > raw_align_size
          },
        n_blk: unc_len.div_ceil(blk_size as u64).max(1).try_into()?,
      });
    }

    let mut manifest_names = pre_a[1..]
      .iter()
      .map(|e| e.arc_name.as_str())
      .collect::<Vec<_>>();

    if is_orbis_ps4 && json.sort_manifest.unwrap_or(true) {
      manifest_names.sort_unstable();
    }

    let manifest = if is_orbis_ps4 {
      manifest_names.join("\0")
    } else {
      manifest_names.join("\n")
    };

    pre_a[0].unc_len = manifest.len() as u64;
    pre_a[0].n_blk = manifest
      .len()
      .div_ceil(blk_size as usize)
      .max(1)
      .try_into()?;

    (manifest, pre_a)
  };

  let (pre_b, unique_files, blk_cnt) = {
    let dedup = json.dedup.unwrap_or(true);
    let mut i_blk = 0u32;
    let mut pre_b = Vec::with_capacity(pre_a.len()); // block index
    let mut unique_files = Vec::with_capacity(pre_a.len()); // ent index
    let mut hash_ent = HashMap::with_capacity(pre_a.len());
    let mut hash_buf = vec![0u8; 1048576];

    for (i, e) in pre_a.iter().enumerate() {
      let j = if dedup {
        let mut hasher = DefaultHasher::new();
        if i == 0 {
          hasher.write(manifest.as_bytes());
        } else {
          let mut file =
            File::open(&e.path).with_context(|| anyhow!("hash {}", e.path.display()))?;
          loop {
            let got = file.read(&mut hash_buf)?;
            if got > 0 {
              hasher.write(&hash_buf[0..got]);
            } else {
              break;
            }
          }
        }

        let entry = hash_ent.entry(hasher.finish()).or_insert(i);
        if *entry != i {
          let j_ent = &pre_a[*entry];
          if !(j_ent.c_force == e.c_force && j_ent.c_level == e.c_level) {
            println!(
              "dedup: comp settings mismatch: {} <- {}",
              j_ent.path.display(),
              e.path.display()
            );
            *entry = i;
          }
        }
        *entry
      } else {
        i
      };
      if j == i {
        unique_files.push(i);
        if pre_a[i].align_raw {
          i_blk = i_blk
            .checked_add(1)
            .ok_or_else(|| anyhow!("total blk count would overflow"))?;
        }
        pre_b.push(i_blk);
        i_blk = i_blk
          .checked_add(e.n_blk)
          .ok_or_else(|| anyhow!("total blk count would overflow"))?;
      } else {
        println!("dedup: {} <- {}", pre_a[j].path.display(), e.path.display());
        pre_b.push(pre_b[j]);
      };
    }

    if pre_a.len() != unique_files.len() {
      println!(
        "dedup: {} files, {} unique files",
        pre_a.len(),
        unique_files.len()
      );
    }

    println!("blocks: size {} x count {}", blk_size, i_blk);

    (pre_b, unique_files, i_blk)
  };

  let bss_type = BssType::from_blksz_cnt(blk_size, blk_cnt);
  let info_len = try {
    bss_type
      .len()?
      .checked_add((pre_a.len() as u64 * 30 + 32).try_into().ok()?)?
  }
  .ok_or_else(|| anyhow!("info len would overflow"))?;

  let mut out_file =
    File::create_buffered(&out_file).with_context(|| anyhow!("create {}", out_file.display()))?;
  out_file.seek(SeekFrom::Start(info_len as u64))?;

  let err = RefCell::new(Result::<()>::Ok(()));
  let mut bos = Vec::with_capacity(blk_cnt as usize);
  let mut bss = Vec::with_capacity(blk_cnt as usize);
  let mut blk_off = info_len as u64;

  for (com_blk, align_raw) in unique_files
    .iter()
    .map_while(|ia| {
      match try {
        let r: Box<dyn Read> = if *ia == 0 {
          Box::new(Cursor::new(manifest.as_bytes()))
        } else {
          let path = &pre_a[*ia].path;
          println!("{}", path.display());
          Box::new(File::open(path).with_context(|| anyhow!("open {}", path.display()))?)
        };
        r
      } {
        Ok(f) => Some((f, &pre_a[*ia])),
        Err(e) => {
          *err.borrow_mut() = Err(e);
          None
        }
      }
    })
    .flat_map(|(mut f, e)| {
      (0..e.n_blk).map(move |ib| {
        let mut blk = Vec::with_capacity(blk_size as usize);
        let got = (&mut f)
          .take(blk_size as u64)
          .read_to_end(&mut blk)
          .with_context(|| anyhow!("read {} block {}", e.path.display(), ib))?;
        blk.resize(got, 0);
        Ok((blk, e.c_force, e.c_level, ib == 0 && e.align_raw))
      })
    })
    .map_while(|res| match res {
      Ok(v) => Some(v),
      Err(e) => {
        *err.borrow_mut() = Err(e);
        None
      }
    })
    .map_parallel(move |(data, c_force, c_level, align_raw)| {
      if c_level < 0 {
        return (data, align_raw);
      }

      let com = match comp.com(&data, c_level as _) {
        Ok(com) => com,
        Err(_) => return (data, align_raw),
      };

      if com.len() < data.len() || (c_force && com.len() <= blk_size as usize) {
        (com, align_raw)
      } else {
        (data, align_raw)
      }
    })
  {
    if err.borrow().is_err() {
      return err.replace(Ok(()));
    }

    if align_raw {
      let padding = (raw_alignment - blk_off % raw_alignment) % raw_alignment;
      bos.push(blk_off);
      bss.push(padding as u32);
      if padding != 0 {
        out_file.write_all(&vec![0; padding as usize])?;
        blk_off += padding;
      }
    }

    bos.push(blk_off);
    bss.push(com_blk.len() as u32);
    blk_off += com_blk.len() as u64;

    out_file.write_all(&com_blk)?;
  }

  let mut ents = Vec::with_capacity(pre_a.len());

  for (ea, blk_idx) in pre_a.into_iter().zip(pre_b) {
    ents.push(Ent {
      name_md5: NameMd5(ea.name_md5.0),
      blk_idx,
      unc_len: ea.unc_len,
      blk_off: bos[blk_idx as usize],
    });
  }

  if is_orbis_ps4 && json.sort_toc.unwrap_or(true) {
    ents[1..].sort_by(|a, b| a.name_md5.0.cmp(&b.name_md5.0));
  }

  out_file.seek(SeekFrom::Start(0))?;

  Info {
    v_maj,
    v_min,
    compr: comp,
    info_len,
    blk_size,
    flags: (if igncase { 1 } else { 0 })
      | (if abspath { 2 } else { 0 })
      | (if is_orbis_ps4 && json.sort_toc.unwrap_or(true) { 4 } else { 0 })
      | (if is_orbis_ps4 && json.sort_manifest.unwrap_or(true) { 8 } else { 0 }),
    igncase,
    abspath,
    ents,
  }
  .write(&mut out_file)?;

  bss_type.write(&mut out_file, &bss)?;

  out_file.flush()?;

  err.into_inner()
}

