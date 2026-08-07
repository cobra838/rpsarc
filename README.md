# rpsarc
A simple psarc utility in rust

## Usage

```
# List files
rpsarc l archive.psarc

# Show header and detected writer profile
rpsarc i archive.psarc

# Export a JSON recipe without extracting files
rpsarc j archive.psarc __manifest.json

# Extract files and write out_dir/__manifest.json
rpsarc x archive.psarc out_dir

# Override profile detection when exporting or extracting
rpsarc j --profile ps3 archive.psarc __manifest.json
rpsarc x --profile orbis_ps4 archive.psarc out_dir

# Create an archive from a JSON recipe
rpsarc c out_dir/__manifest.json rebuilt.psarc
```

## Manifest

```javascript
{
  // Writer profile: ps3 or orbis_ps4
  "profile": "ps3",
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

### Orbis / PS4 options

Set the common `profile` field to `"orbis_ps4"` to use the Orbis writer. It
additionally accepts these fields:

```javascript
{
  "profile": "orbis_ps4",
  // Header flag 0x04: sort TOC entries by name MD5.
  "sort_toc": true,
  // Header flag 0x08: sort manifest names; uses NUL separators.
  // false keeps input order and uses LF separators.
  "sort_manifest": true,
  // Compress the internal filename manifest with the selected zlib level.
  "compress_manifest": true,
  // Raw files at least this large are aligned before storage.
  "file_align_size": 2097152,
  "file_alignment": 65536
}
```

The Orbis defaults are `sort_toc: true`, `sort_manifest: true`,
`file_align_size: 2097152`, and `file_alignment: 65536`.
