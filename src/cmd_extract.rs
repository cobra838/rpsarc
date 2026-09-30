use crate::{
  bin_format::{BssType, Ent, Info},
  mani_json::{ManiFile, ManiJson},
};
use anyhow::{Result, anyhow};
use binrw::BinRead;
use md5::Digest;
use parseq::ParallelIterator;
use std::{
  collections::{HashMap, HashSet},
  fs::{File, create_dir_all},
  io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
  path::PathBuf,
};

struct Loaded {
  file: BufReader<File>,
  header: Info,
  bss: Vec<u32>,
  names: Vec<String>,
  profile: Profile,
  recipe: ManiJson,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Profile {
  Ps3,
  OrbisPs4,
}

impl Profile {
  const fn name(self) -> &'static str {
    match self {
      Self::Ps3 => "ps3",
      Self::OrbisPs4 => "orbis_ps4",
    }
  }

  const fn is_orbis(self) -> bool {
    matches!(self, Self::OrbisPs4)
  }
}

fn has_orbis_alignment_layout(header: &Info, bss_raw: &[u32]) -> bool {
  let mut referenced = HashSet::with_capacity(bss_raw.len());
  let mut unique = HashSet::with_capacity(header.ents.len() - 1);
  let mut orbis_spares = 0usize;
  let mut ps3_spares = 0usize;

  for (index, entry) in header.ents.iter().enumerate() {
    let blocks = entry.unc_len.div_ceil(header.blk_size as u64).max(1) as u32;
    for block in entry.blk_idx..entry.blk_idx + blocks {
      referenced.insert(block);
    }
    if index == 0 || !unique.insert((entry.blk_idx, entry.blk_off, entry.unc_len)) {
      continue;
    }

    let mut remaining = entry.unc_len;
    let raw = (0..blocks as usize).all(|block| {
      let expected = remaining.min(header.blk_size as u64) as u32;
      remaining -= expected as u64;
      let stored = bss_raw[entry.blk_idx as usize + block];
      (if stored == 0 { header.blk_size } else { stored }) == expected
    });
    if raw {
      if entry.unc_len >= 2_097_152 {
        orbis_spares += 1;
      }
      if entry.unc_len > 65_536 {
        ps3_spares += 1;
      }
    }
  }

  let spare_count = bss_raw.len().saturating_sub(referenced.len());
  spare_count > 0 && spare_count == orbis_spares && spare_count != ps3_spares
}

fn detect_profile(header: &Info, bss_raw: &[u32], manifest: &str) -> Profile {
  // 0x04 = sorttoc and 0x08 = sortmanifest are Orbis writer flags.
  // Orbis manifests also use NUL separators, unlike the PS3 LF form.
  if header.flags & 0x0C != 0
    || manifest.contains('\0')
    || has_orbis_alignment_layout(header, bss_raw)
  {
    Profile::OrbisPs4
  } else {
    Profile::Ps3
  }
}

fn parse_profile_override(profile: Option<&str>) -> Result<Option<Profile>> {
  match profile {
    None => Ok(None),
    Some("ps3") => Ok(Some(Profile::Ps3)),
    Some("orbis_ps4") => Ok(Some(Profile::OrbisPs4)),
    Some(value) => Err(anyhow!("unknown profile {value}; expected ps3 or orbis_ps4")),
  }
}

// PS3 PSARC stores names separated by LF. Orbis PSARC uses NUL instead (and does not require a final terminator).
fn parse_manifest_names(manifest: &str) -> Vec<String> {
  let separator = if manifest.contains('\0') { '\0' } else { '\n' };
  manifest
    .split(separator)
    .filter(|name| !name.is_empty())
    .map(str::to_owned)
    .collect()
}

fn load(in_file: PathBuf, profile_override: Option<&str>) -> Result<Loaded> {
  let mut file = File::open_buffered(in_file)?;
  let header = Info::read(&mut file)?;
  let bss_raw = BssType::from_info(&header)?.read_raw(&mut file)?;
  let bss = bss_raw
    .iter()
    .map(|&v| if v == 0 { header.blk_size } else { v })
    .collect::<Vec<_>>();
  let entry = &header.ents[0];
  let blocks = entry.unc_len.div_ceil(header.blk_size as u64).max(1) as usize;
  let mut bytes = Vec::with_capacity(entry.unc_len as usize);
  file.seek(SeekFrom::Start(entry.blk_off))?;
  for &size in &bss[entry.blk_idx as usize..entry.blk_idx as usize + blocks] {
    let mut block = vec![0; size as usize];
    file.read_exact(&mut block)?;
    bytes.extend(
      header
        .compr
        .dec(&block, header.blk_size as usize)
        .unwrap_or(block),
    );
  }
  bytes.truncate(entry.unc_len as usize);
  let manifest = String::from_utf8(bytes)?;
  let profile = parse_profile_override(profile_override)?
    .unwrap_or_else(|| detect_profile(&header, &bss_raw, &manifest));
  let manifest_names = parse_manifest_names(&manifest);
  if manifest_names.len() + 1 != header.ents.len() {
    return Err(anyhow!("manifest name count does not match TOC"));
  }

  let mut by_md5 = HashMap::with_capacity(manifest_names.len());
  for name in &manifest_names {
    let md5 = if header.igncase {
      md5::compute(name.to_ascii_uppercase()).0
    } else {
      md5::compute(name).0
    };
    if let Some(existing) = by_md5.get(&md5) {
      if existing != name {
        return Err(anyhow!(
          "filename MD5 collision in manifest: {existing} and {name}"
        ));
      }
    } else {
      by_md5.insert(md5, name.clone());
    }
  }

  // PS3 usually keeps manifest and TOC order. Orbis sort options can change either order, so resolve names by MD5.
  let names = header.ents[1..]
    .iter()
    .map(|entry| {
      by_md5
        .get(&entry.name_md5.0)
        .cloned()
        .ok_or_else(|| anyhow!("TOC MD5 {} is absent from manifest", entry.name_md5))
    })
    .collect::<Result<Vec<_>>>()?;
  let compressed = |entry: &Ent| {
    if entry.unc_len == 0 {
      return false;
    }
    let mut remaining = entry.unc_len;
    (0..entry.unc_len.div_ceil(header.blk_size as u64).max(1) as usize).any(|i| {
      let expected = remaining.min(header.blk_size as u64) as u32;
      remaining -= expected as u64;
      let stored = bss_raw[entry.blk_idx as usize + i];
      (if stored == 0 { header.blk_size } else { stored }) != expected
    })
  };
  let manifest_compressed = compressed(&header.ents[0]);

  let mut used_blocks = HashSet::with_capacity(header.ents.len() - 1);
  let dedup = header.ents[1..]
    .iter()
    .any(|entry| !used_blocks.insert((entry.blk_idx, entry.blk_off)));

  let stored_by_name = names
    .iter()
    .zip(&header.ents[1..])
    .map(|(name, entry)| (name.as_str(), compressed(entry)))
    .collect::<HashMap<_, _>>();

  let mut data_order = header.ents[1..].iter().zip(&names).collect::<Vec<_>>();
  data_order.sort_by_key(|(entry, _)| entry.blk_off);

  let file_names = if profile.is_orbis() && header.flags & 0x08 != 0 {
    // A sorted Orbis manifest loses archive input order; use physical order.
    data_order
      .into_iter()
      .map(|(_, name)| name)
      .collect::<Vec<_>>()
  } else {
    // An unsorted manifest preserves input and dedup order.
    manifest_names.iter().collect::<Vec<_>>()
  };
  let files = file_names
    .into_iter()
    .map(|name| ManiFile {
      path: name.strip_prefix('/').unwrap_or(&name).to_owned(),
      // Preserve the manifest name for absolute-path archives.
      name: header.abspath.then_some(name.to_owned()),
      compressed: Some(
        *stored_by_name
          .get(name.as_str())
          .expect("name resolved from manifest"),
      ),
      compression_level: None,
      force_comp: None,
    })
    .collect::<Vec<_>>();

  let compression_enabled =
    manifest_compressed || files.iter().any(|file| file.compressed == Some(true));
  let recipe = ManiJson {
    profile: Some(profile.name().to_owned()),
    ver_maj: header.v_maj,
    ver_min: header.v_min,
    compression: header.compr.name().to_owned(),
    compression_enabled: Some(compression_enabled),
    compression_level: Some(9),
    force_comp: None,
    block_size: Some(header.blk_size),
    ignorecase: Some(header.igncase),
    absolute: Some(header.abspath),
    dedup: Some(dedup),
    compress_manifest: Some(manifest_compressed),
    sort_toc: profile.is_orbis().then_some(header.flags & 0x04 != 0),
    sort_manifest: profile.is_orbis().then_some(header.flags & 0x08 != 0),
    file_align_size: Some(if profile.is_orbis() { 2097152 } else { 65536 }),
    file_alignment: Some(if profile.is_orbis() { 65536 } else { 8192 }),
    files,
  };
  Ok(Loaded {
    file,
    header,
    bss,
    names,
    profile,
    recipe,
  })
}

pub fn inspect(in_file: PathBuf) -> Result<()> {
  let loaded = load(in_file, None)?;
  let header = &loaded.header;
  let profile = match loaded.profile {
    Profile::OrbisPs4 => "orbis_ps4 (likely)",
    Profile::Ps3 => "ps3 (likely)",
  };
  println!("Profile: {profile}");
  println!(
    "PSAR {}.{}, {}, block size {}, flags 0x{:08X}",
    header.v_maj,
    header.v_min,
    header.compr.name(),
    header.blk_size,
    header.flags
  );
  Ok(())
}

pub fn export_json(in_file: PathBuf, out_json: PathBuf, profile: Option<String>) -> Result<()> {
  let loaded = load(in_file, profile.as_deref())?;
  File::create(out_json)?.write_all(&serde_json::to_vec_pretty(&loaded.recipe)?)?;
  Ok(())
}

pub fn extract(
  in_file: PathBuf,
  out_dir: PathBuf,
  list_only: bool,
  profile: Option<String>,
  includes: Vec<String>,
) -> Result<()> {
  let Loaded {
    mut file,
    header: hdr,
    bss,
    names,
    recipe,
    ..
  } = load(in_file, profile.as_deref())?;

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

  let mut namelist = Vec::with_capacity(hdr.ents.len());
  namelist.push("__manifest.txt".to_owned());
  namelist.extend(
    names
      .into_iter()
      .map(|name| name.strip_prefix('/').unwrap_or(&name).to_owned()),
  );

  if list_only {
    for (entry, name) in hdr.ents[1..].iter().zip(&namelist[1..]) {
      let blocks = entry.unc_len.div_ceil(hdr.blk_size as u64).max(1) as usize;
      let stored = if entry.unc_len == 0 {
        0
      } else {
        bss[entry.blk_idx as usize..entry.blk_idx as usize + blocks]
          .iter()
          .map(|&size| size as u64)
          .sum()
      };
      let percent = if entry.unc_len == 0 {
        100
      } else {
        stored * 100 / entry.unc_len
      };
      println!(
        "name_md5={:x} : {name} ({stored}/{}/{}%)",
        Digest(entry.name_md5.0),
        entry.unc_len,
        percent
      );
    }
    return Ok(());
  }

  let selected = namelist
    .iter()
    .enumerate()
    .map(|(index, name)| {
      index != 0
        && (includes.is_empty()
          || includes
            .iter()
            .any(|include| matches_include(name, include)))
    })
    .collect::<Vec<_>>();
  let (block_infos, empty_files) = calc_blocks_info(bss, hdr.ents, hdr.blk_size, &selected)?;

  for i in empty_files {
    println!("empty file {}", namelist[i]);
    let p = out_dir.join(&namelist[i]);
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
      // A block whose stored and uncompressed sizes match is raw. Do not feed it to zlib: arbitrary raw bytes can otherwise be accepted as an empty/partial zlib stream and silently extract as zero bytes.
      let dat = if bi.len == bi.unc_len {
        blk
      } else {
        comp.dec(&blk, bi.unc_len as usize).unwrap_or(blk)
      };
      (dat, bi.f_write, bi.f_close)
    })
  {
    for i in wf {
      let f = &mut filelist[i];
      match f {
        Some(w) => w.write_all(&dat)?,
        None => {
          let p = out_dir.join(&namelist[i]);
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

  if includes.is_empty() {
    File::create(out_dir.join("__manifest.json"))?
      .write_all(&serde_json::to_vec_pretty(&recipe)?)?;
  }

  Ok(())
}

fn matches_include(path: &str, include: &str) -> bool {
  let path = path.trim_start_matches('/');
  let include = include.replace('\\', "/");
  let include = include.trim_start_matches('/');
  if include.ends_with('/') {
    path.starts_with(include)
  } else if let Some(suffix) = include.strip_prefix("**/*") {
    path.ends_with(suffix)
  } else if let Some(suffix) = include.strip_prefix('*') {
    path.ends_with(suffix)
  } else {
    path == include
      || path
        .strip_prefix(include)
        .is_some_and(|rest| rest.starts_with('/'))
  }
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
  selected: &[bool],
) -> Result<(Vec<BlkInfo>, Vec<usize>)> {
  let mut infos = bss
    .into_iter()
    .map(|len| BlkInfo {
      off: 0,
      len,
      unc_len: 0,
      f_write: vec![],
      f_close: vec![],
    })
    .collect::<Vec<_>>();

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
    // Entry 0 is the internal PSARC filename manifest. It is parsed into __manifest.json and is not an extracted user file.
    if i == 0 || !selected[i] {
      continue;
    }
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

  // The internal manifest and alignment padding can leave ZSize entries with no extracted file block.
  infos.retain(|bi| bi.off != 0);
  Ok((infos, empty))
}
