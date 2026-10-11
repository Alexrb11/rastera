use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=resources/rastera.rc");
    println!("cargo:rerun-if-changed=../../assets/branding/rastera.ico");
    println!("cargo:rerun-if-env-changed=RC");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let target = env::var("TARGET").expect("missing build target");
    println!("cargo:rerun-if-env-changed=RC_{target}");
    println!("cargo:rerun-if-env-changed=RC_{}", target.replace('-', "_"));

    let crate_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("missing crate directory"));
    let branding_dir = crate_dir.join("../../assets/branding");
    assert!(
        branding_dir.join("rastera.ico").is_file(),
        "the Windows application icon is missing: assets/branding/rastera.ico"
    );

    // Resolve resource inputs independently of the working directory or SDK include paths.
    // A Windows build must fail if RC/windres cannot embed the approved application icon.
    embed_resource::compile_for(
        "resources/rastera.rc",
        ["rastera"],
        embed_resource::ParamsIncludeDirs([branding_dir]),
    )
    .manifest_required()
    .expect("failed to embed the Windows icon; install the Windows SDK (MSVC) or windres (GNU)");
}
