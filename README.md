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

```javascript
{
  // PSARC version
  "ver_maj": 1,
  "ver_min": 4,
  // Available: zlib, lzma (not implemented)
  "compression": "zlib",
  // zlib: zenflate effort, -1: store uncompressed
  "compr_level": 16,
  // prefer compressed data, even if it's larger than original (while still smaller than block_size)
  "force_comp": false,
  // block_size: power of two
  "block_size": 65536,
  // ignore case: convert to upper when calculating name md5
  "ignorecase": false,
  // absolute path: prepend '/' to paths
  "absolute": false,
  // deduplicate: store identical files only once (compr_level and force_comp should equal)
  "dedup": true,
  "files": [
    {
      "path": "Data/PSVita/Character/EXBG_COMMON_DS_V_01/EXBG_COMMON_DS_V_01.elixir.gz",
      // optional: override name (in manifest and name md5 calculation), force_comp and compr_level
      "name": "EXBG_COMMON_DS_V_01.elixir.gz",
      "compr_level": 24,
      "force_comp": true,
    },
  ]
}
```