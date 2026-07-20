// On ELF (Linux) systems, if a shared library is linked by absolute path
// without a recorded SONAME, the consumer records DT_NEEDED as the
// absolute path. Any path containing a slash bypasses RPATH/RUNPATH search
// at runtime (per ld.so(8)), so the consumer will fail with "cannot open
// shared object file" when moved off the build machine.
//
// Embedding a bare SONAME into the cdylib fixes this: the linker records
// DT_NEEDED as the SONAME (bare filename), and the runtime loader consults
// RPATH/RUNPATH to find it. macOS uses install_name (a different mechanism
// handled by rustc automatically) and Windows doesn't have SONAMEs at all.
fn main() {
    if cfg!(all(unix, not(target_os = "macos"))) {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libqirtoqasm.so");
    }
}
