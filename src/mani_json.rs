use serde::{Deserialize, Serialize};

/// A build recipe
#[derive(Serialize, Deserialize)]
pub struct ManiJson {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub profile: Option<String>,
  pub ver_maj: u16,
  pub ver_min: u16,
  pub compression: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compression_enabled: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compression_level: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub force_comp: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub block_size: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ignorecase: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub absolute: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub dedup: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compress_manifest: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sort_toc: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sort_manifest: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub file_align_size: Option<u64>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub file_alignment: Option<u32>,
  pub files: Vec<ManiFile>,
}

#[derive(Serialize, Deserialize)]
pub struct ManiFile {
  pub path: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compressed: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compression_level: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub force_comp: Option<bool>,
}
