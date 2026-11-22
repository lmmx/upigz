fn main() {
    cc::Build::new().file("csrc/isal_shim.c").opt_level(3).compile("isal_shim");
    println!("cargo:rustc-link-lib=isal");
    println!("cargo:rerun-if-changed=csrc/isal_shim.c");
}
