use std::path::PathBuf;

fn main() {
  let root = PathBuf::from("vendor/zlib-1.2.3");
  let sources = [
    "adler32.c", "compress.c", "crc32.c", "deflate.c", "infback.c",
    "inffast.c", "inflate.c", "inftrees.c", "trees.c", "uncompr.c", "zutil.c",
  ];
  let mut build = cc::Build::new();
  build.include(&root).warnings(false);
  for source in sources {
    let path = root.join(source);
    build.file(&path);
    println!("cargo:rerun-if-changed={}", path.display());
  }
  println!("cargo:rerun-if-changed={}", root.join("zlib.h").display());
  println!("cargo:rerun-if-changed={}", root.join("zconf.h").display());
  build.compile("zlib123");
}
