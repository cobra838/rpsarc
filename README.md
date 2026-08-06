# rpsarc
A simple psarc utility in rust

## Usage

```
# List files
rpsarc l in_file.psarc

# Extract an archive, out_dir/__manifest.json is for recreating
rpsarc x in_file.psarc out_dir

# Create an archive
rpsarc c out_dir/__manifest.json new_file.psarc
```

## Manifest

`rpsarc x` writes `__manifest.json`.  For an archive that can be reproduced
without recompression it is also a repack recipe: it contains the raw flags,
ZSize table, offsets, storage mode and layout.  `rpsarc c` preserves that
layout when `layout.mode` is `preserve`.

For a manually authored manifest, or one without `layout`, `rpsarc c` uses the
normal build mode below.  `compression_level` and `force_comp` are writer options;
they are not recovered from an existing archive.

```javascript
{
  // PSARC version
  "ver_maj": 1,
  "ver_min": 4,
  // Available: zlib, lzma (not implemented)
  "compression": "zlib",
  "compression_enabled": true,
  // zlib level, 1 through 9
  "compression_level": 9,
  // prefer compressed data, even if it's larger than original (while still smaller than block_size)
  "force_comp": false,
  // block_size: power of two
  "block_size": 65536,
  // ignore case: convert to upper when calculating name md5
  "ignorecase": false,
  // absolute path: prepend '/' to paths
  "absolute": false,
  // deduplicate: store identical files only once (compression settings should equal)
  "dedup": true,
  "files": [
    {
      "path": "Data/PSVita/Character/EXBG_COMMON_DS_V_01/EXBG_COMMON_DS_V_01.elixir.gz",
      // optional: override name (in manifest and name md5 calculation), force_comp and compression_level
      "name": "EXBG_COMMON_DS_V_01.elixir.gz",
      "compression_level": 9,
      "force_comp": true,
    },
  ]
}
```
