fn main() {
  let deps_dir = "./deps";
  println!("cargo:rustc-link-search=native={}", deps_dir);
  println!("cargo:rustc-link-lib=dylib=tibems64");
  println!("cargo:rustc-link-lib=dylib=tibemsadmin64");
}