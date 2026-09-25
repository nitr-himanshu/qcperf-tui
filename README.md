# qcperf-tui

An interactive Rust-based TUI for libqcperf that makes performance profiling, trace exploration, and metric analysis accessible directly from the terminal.

libqcperf is included as a git submodule at `third_party/libqcperf`. Its own build instructions are in [third_party/libqcperf/README.md](third_party/libqcperf/README.md).

## Prerequisites

### All targets

- [Git](https://git-scm.com/)
- [Rust](https://rustup.rs/) (stable), installed with rustup
- [CMake](https://cmake.org/) 3.15 or newer (required by libqcperf)
- This repo's submodules, including libqcperf and its third-party dependencies:

```bash
git submodule update --init --recursive
```

Add the Rust target you are building for:

```bash
rustup target add aarch64-unknown-linux-gnu   # Linux ARM64
rustup target add aarch64-linux-android       # Android ARM64
rustup target add aarch64-pc-windows-msvc     # Windows ARM64
```

### Linux ARM64 (`aarch64-unknown-linux-gnu`)

From the [libqcperf Linux ARM64 instructions](third_party/libqcperf/README.md#linux-arm64-compilation):

- [ARM GNU Toolchain](https://developer.arm.com/downloads/-/arm-gnu-toolchain-downloads) (`aarch64-none-linux-gnu-gcc`)
- `AARCH64_TOOLCHAIN_PATH` set to that toolchain's install directory
- For the NPU backend: the [fastrpc](https://github.com/qualcomm/fastrpc) submodule at `third_party/libqcperf/qcperf/third-party/fastrpc`, plus Autotools (`autoconf`, `automake`, `libtool`) and `make` to build `libcdsprpc.so`

The TUI does not name backends. libqcperf is built with `-DBACKENDS` unset, so every backend that library supports on Linux ARM64 is linked and discovered at startup. Set `QCPERF_BACKENDS` (for example `CPU;NPU`) only to build a subset.

### Android ARM64 (`aarch64-linux-android`)

From the [libqcperf Android ARM64 instructions](third_party/libqcperf/README.md#android-arm64-compilation):

- [Android NDK](https://developer.android.com/ndk/downloads) r24 or newer (`arm64-v8a`, API 29+)
- For the NPU backend: the same fastrpc submodule and Autotools as Linux ARM64
- On a device, if `libcdsprpc.so` is not found at runtime, set `LD_LIBRARY_PATH=/vendor/lib64`

Same discovery rule as Linux ARM64: every Android-supported libqcperf backend is linked unless `QCPERF_BACKENDS` narrows the build.

### Windows ARM64 (`aarch64-pc-windows-msvc`)

From the [libqcperf Windows ARM64 instructions](third_party/libqcperf/README.md#windows-arm64-compilation):

- Visual Studio 2022 with the MSVC ARM64 build tools (CMake generator `Visual Studio 17 2022`, platform `ARM64`)
- Git submodules initialized recursively (libqcperf pulls its own third-party dependencies)

Same discovery rule: every Windows ARM64 backend libqcperf enables is linked unless `QCPERF_BACKENDS` narrows the build. A backend added in that library shows up in the TUI after a rebuild, without a change in this repo's Rust sources.
