//! Build libqcperf from the submodule and generate FFI bindings.
//!
//! Supported targets:
//! - aarch64-unknown-linux-gnu
//! - aarch64-linux-android
//! - aarch64-pc-windows-msvc
//!
//! `-DBACKENDS` is left empty unless `QCPERF_BACKENDS` is set, so every backend
//! libqcperf enables for the platform is linked. New static libraries produced
//! by that build are picked up by scanning the CMake output directory.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let target = env::var("TARGET").expect("TARGET");
    let manifest_dir = PathBuf::from(env::var("MANIFEST_DIR").expect("MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let qcperf_root = manifest_dir.join("third_party").join("libqcperf");
    let source = qcperf_root.join("qcperf");

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-env-changed=QCPERF_BACKENDS");
    println!("cargo:rerun-if-env-changed=AARCH64_TOOLCHAIN_PATH");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_ROOT");
    println!("cargo:rerun-if-env-changed=NDK");

    if !matches!(
        target.as_str(),
        "aarch64-unknown-linux-gnu" | "aarch64-linux-android" | "aarch64-pc-windows-msvc"
    ) {
        panic!(
            "qcperf-tui links libqcperf only for aarch64-unknown-linux-gnu, \
             aarch64-linux-android, and aarch64-pc-windows-msvc (current target: {target})"
        );
    }

    if target != "aarch64-pc-windows-msvc" {
        let status = Command::new("git")
            .args(["submodule", "update", "--init", "--recursive"])
            .current_dir(&qcperf_root)
            .status()
            .expect("failed to run git submodule for libqcperf third-party dependencies");
        if !status.success() {
            panic!("git submodule update failed inside third_party/libqcperf");
        }
    }

    let mut build_dir =
        PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into()))
            .join("libqcperf")
            .join(&target);
    if !build_dir.is_absolute() {
        build_dir = manifest_dir.join(build_dir);
    }
    configure_and_build(&target, &source, &build_dir);
    link_static_libs(&target, &build_dir);
    if target != "aarch64-pc-windows-msvc" {
        link_fastrpc(&source);
    }

    generate_bindings(&source, &out_dir);
    println!("cargo:rustc-cfg=qcperf_static");
}

fn configure_and_build(target: &str, source: &Path, build_dir: &Path) {
    fs::create_dir_all(build_dir).expect("create libqcperf build directory");

    let mut configure = Command::new("cmake");
    configure.arg(format!("-S{}", source.display()));
    configure.arg(format!("-B{}", build_dir.display()));
    configure.arg("-DBUILD_SHARED=OFF");
    configure.arg("-DProjectVersion=0.1.0.0");

    match target {
        "aarch64-unknown-linux-gnu" => {
            if env::var_os("AARCH64_TOOLCHAIN_PATH").is_none() {
                panic!(
                    "Set AARCH64_TOOLCHAIN_PATH to the ARM GNU toolchain root \
                     (the directory that contains bin/aarch64-none-linux-gnu-gcc)"
                );
            }
            configure.arg("-DTARGET_ARCH=linux-aarch64");
            configure.arg("-DCMAKE_BUILD_TYPE=Release");
        }
        "aarch64-linux-android" => {
            let ndk = ndk_path();
            let toolchain = ndk.join("build").join("cmake").join("android.toolchain.cmake");
            configure.arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()));
            configure.arg("-DANDROID_ABI=arm64-v8a");
            configure.arg("-DANDROID_PLATFORM=android-29");
            configure.arg("-DCMAKE_BUILD_TYPE=Release");
        }
        "aarch64-pc-windows-msvc" => {
            configure.arg("-G");
            configure.arg("Visual Studio 17 2022");
            configure.arg("-A");
            configure.arg("ARM64");
        }
        _ => unreachable!(),
    }

    if let Ok(backends) = env::var("QCPERF_BACKENDS") {
        configure.arg(format!("-DBACKENDS={backends}"));
    } else {
        configure.arg("-DBACKENDS=");
    }

    let status = configure.status().expect("failed to spawn cmake");
    if !status.success() {
        panic!("cmake configure failed for libqcperf");
    }

    let mut build = Command::new("cmake");
    build.arg("--build").arg(build_dir);
    if target == "aarch64-pc-windows-msvc" {
        build.arg("--config").arg("Release");
    }
    let status = build.status().expect("failed to spawn cmake --build");
    if !status.success() {
        panic!("cmake build failed for libqcperf");
    }
}

fn link_fastrpc(source: &Path) {
    let libs = source
        .join("third-party")
        .join("fastrpc")
        .join("src")
        .join(".libs");
    let shared = libs.join("libcdsprpc.so");
    if !shared.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", shared.display());
    println!("cargo:rustc-link-search=native={}", libs.display());
    println!("cargo:rustc-link-lib=dylib=cdsprpc");
}

fn ndk_path() -> PathBuf {
    for key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK"] {
        if let Ok(value) = env::var(key) {
            return PathBuf::from(value);
        }
    }
    panic!("Set ANDROID_NDK_HOME to the Android NDK r24+ root");
}

fn link_static_libs(target: &str, build_dir: &Path) {
    let mut libs = Vec::new();
    collect_static_libs(build_dir, &mut libs);
    if libs.is_empty() {
        panic!(
            "libqcperf build produced no static libraries under {}",
            build_dir.display()
        );
    }
    libs.sort();
    let mut search = Vec::new();
    for lib in &libs {
        if let Some(dir) = lib.parent() {
            if !search.contains(&dir.to_path_buf()) {
                search.push(dir.to_path_buf());
            }
        }
    }
    for dir in search {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    for lib in &libs {
        let name = lib
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("static library name");
        let name = name.strip_prefix("lib").unwrap_or(name);
        println!("cargo:rustc-link-lib=static:+whole-archive={name}");
    }

    match target {
        "aarch64-unknown-linux-gnu" => {
            println!("cargo:rustc-link-lib=stdc++");
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=m");
        }
        "aarch64-linux-android" => {
            println!("cargo:rustc-link-lib=c++");
            println!("cargo:rustc-link-lib=log");
            println!("cargo:rustc-link-lib=dl");
            println!("cargo:rustc-link-lib=m");
        }
        "aarch64-pc-windows-msvc" => {
            println!("cargo:rustc-link-lib=pdh");
            println!("cargo:rustc-link-lib=wbemuuid");
            println!("cargo:rustc-link-lib=ole32");
            println!("cargo:rustc-link-lib=oleaut32");
            println!("cargo:rustc-link-lib=advapi32");
        }
        _ => unreachable!(),
    }
}

fn collect_static_libs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.to_string_lossy();
            if name.contains("Debug") || name.contains("MinSizeRel") || name.contains("RelWithDebInfo") {
                continue;
            }
            collect_static_libs(&path, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if ext.eq_ignore_ascii_case("a") || ext.eq_ignore_ascii_case("lib") {
            out.push(path);
        }
    }
}

fn generate_bindings(source: &Path, out_dir: &Path) {
    let include = out_dir.join("include");
    fs::create_dir_all(&include).expect("create bindgen include dir");
    fs::write(
        include.join("version_info.h"),
        r#"#ifndef LIBQCPERF_VERSION_INFO_H
#define LIBQCPERF_VERSION_INFO_H
#include <stdint.h>
#define LIBQCPERF_VERSION_BUILD 0
#define LIBQCPERF_VERSION_MAJOR 1
#define LIBQCPERF_VERSION_MINOR 0
#define LIBQCPERF_VERSION_PATCH 0
struct QcPerfVersionInfo {
    uint8_t build;
    uint8_t major;
    uint8_t minor;
    uint8_t patch;
};
#define LIBQCPERF_BUILD_TIMESTAMP "unknown"
#define LIBQCPERF_GIT_HASH "unknown"
#endif
"#,
    )
    .expect("write version_info.h");

    let wrapper = out_dir.join("wrapper.h");
    fs::write(&wrapper, "#include \"qcperf.h\"\n").expect("write wrapper.h");

    let bindings = bindgen::Builder::default()
        .header(wrapper.to_string_lossy())
        .clang_arg("-DQCPERF_STATIC_LIBRARY")
        .clang_arg(format!("-I{}", include.display()))
        .clang_arg(format!("-I{}", source.join("core").join("inc").display()))
        .clang_arg(format!(
            "-I{}",
            source.join("backends").join("inc").display()
        ))
        .allowlist_function("qcperf_.*")
        .allowlist_type("QcPerf.*")
        .allowlist_var("QC_PERF_.*")
        .allowlist_var("MAX_.*")
        .allowlist_var("CAPABILITY_NAME_MAX_LEN")
        .allowlist_var("METRIC_NAME_MAX_LEN")
        .allowlist_var("RETURN_CODE_INFO_STRING_MAX_LEN")
        .allowlist_var("ERROR_STRING_MAX_LEN")
        .rustified_enum("QcPerfReturnCode")
        .rustified_enum("QcPerfDataType")
        .rustified_enum("QcPerfMessageLevel")
        .newtype_enum("QcPerfBackendId")
        .layout_tests(false)
        .generate()
        .expect("bindgen failed on libqcperf headers");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("write bindings.rs");
}
