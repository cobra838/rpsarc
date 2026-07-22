use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ManiJson<'a> {
  pub ver_maj: u16,
  pub ver_min: u16,
  pub compression: &'a str,
  pub compr_level: i32,
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
  pub files: Vec<ManiFile<'a>>,
}

#[derive(Serialize, Deserialize)]
pub struct ManiFile<'a> {
  pub path: &'a str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<&'a str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub compr_level: Option<i32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub force_comp: Option<bool>,
}
