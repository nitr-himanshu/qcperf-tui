# qcperf-tui

Terminal UI for [libqcperf](https://github.com/qualcomm/libqcperf). It profiles Qualcomm performance metrics on device and draws them as live charts in the terminal.

libqcperf ships in this repo as the git submodule `third_party/libqcperf`. The TUI initializes that library, connects every backend the build enabled, and reads capabilities at startup. Charts, rates, and the default dashboard all come from those capability records. Adding a backend inside libqcperf does not require a matching change in this crate: the next build links it, and the next launch lists its metrics.

## Overview

`qcperf-tui` is a dashboard for counters libqcperf already knows how to sample. On launch it calls `qcperf_init`, walks backend ids up to `QC_PERF_BACKEND_MAX`, and keeps every backend that connects. Each capability reports its metrics, units, and the sampling and streaming periods it accepts.

An **Overview** dashboard is created the first time the program runs. Percent metrics open as pies of the current value. Frequencies and every other unit open as scrolling line charts over a time window, in the same spirit as Task Manager. From there you build more dashboards, choose which metrics each one shows, and how each metric is drawn.

Profiling belongs to the dashboard, not to the screen in front. Switching dashboards leaves the others running. Two dashboards that select the same capability at the same rates share one `qcperf_start`. Stop applies only to the dashboard you are looking at.

The layout follows the terminal size: one column under 60 columns, two up to 119, and three from 120 up.

## Features

- Live graphs fed by libqcperf data callbacks. Numeric samples are kept; string values are not plotted.
- Backend discovery from the linked library. The UI does not hard-code CPU, NPU, thermal, or power.
- Default **Overview** dashboard built from whatever capabilities connected.
- Custom dashboards. Each metric can be a pie, a line, or a bar.
- Percent units default to a pie. Several percent metrics from the same capability share one pie. Other units default to a line. `MHz` and `Hz` are lines.
- Per-capability sampling and streaming rates, chosen only from the lists libqcperf returned for that capability. The rate applies to every graph of that capability.
- Several dashboards can profile at the same time. Leaving a dashboard does not stop it.
- Start and stop per dashboard. The last dashboard to stop a shared capability is the one that calls `qcperf_stop`.
- Rolling history. Each graph has a visible window (15 s, 30 s, 60 s, or 5 min). Samples outside the window scroll off.
- Colors from `colors.toml`, with a per-series override in the editor.
- CSV export of the current rings, and optional append-while-running.
- JSON snapshot of the current rings. A snapshot does not stop profiling.
- Dashboards are stored as TOML and restored on the next launch in the Idle state. Press Start to profile again.

### Keys

Press `?` in the list or the live view for the same map.

| Context | Key | Action |
|---|---|---|
| List | Up / Down | Select a dashboard |
| List | Enter | Open it |
| List | `n` | New dashboard |
| List | `d` | Delete a dashboard that is not running |
| List | `q` | Quit |
| Live view | `s` / `x` | Start / stop this dashboard |
| Live view | `e` | Edit metrics, chart, color, window, and rates |
| Live view | Tab, Left, Right | Switch dashboard; profiling continues |
| Live view | `c` | Write current samples to CSV |
| Live view | `v` | Toggle appending CSV while this dashboard runs |
| Live view | `p` | Write a JSON snapshot |
| Live view | Esc | Back to the list |
| Editor | Space | Toggle the focused metric |
| Editor | `t` | Cycle pie, line, bar |
| Editor | `c` | Next color |
| Editor | `w` | Next time window |
| Editor | Left / Right | Step the sampling rate |
| Editor | Shift+Left / Shift+Right | Step the streaming rate |
| Editor | Enter / Esc | Save / cancel |

A rate change on a capability that another running dashboard is already streaming is refused. A rate change on a capability this dashboard alone is streaming is saved and applied the next time you Stop and Start.

### Files

Config lives next to the executable, in a `config` directory. Override that location with `QCPERF_TUI_CONFIG`.

| Path | Contents |
|---|---|
| `config/colors.toml` | Series palette (RGB) |
| `config/dashboards/<id>.toml` | Saved dashboards |
| `config/exports/<name>-<date>.csv` | CSV export |
| `config/snapshots/<name>-<time>.json` | Snapshots |

## Prerequisites

### All targets

- [Git](https://git-scm.com/)
- [Rust](https://rustup.rs/) (stable), installed with rustup
- [CMake](https://cmake.org/) 3.15 or newer (required by libqcperf)
- [libclang](https://clang.llvm.org/), so `bindgen` can read the libqcperf headers
- This repo's submodules, including libqcperf and its third-party dependencies:

```bash
git clone --recurse-submodules https://github.com/nitr-himanshu/qcperf-tui.git
cd qcperf-tui
git submodule update --init --recursive
```

Install the Rust target you are building:

```bash
rustup target add aarch64-unknown-linux-gnu   # Linux ARM64
rustup target add aarch64-linux-android       # Android ARM64
rustup target add aarch64-pc-windows-msvc     # Windows ARM64
```

The crate links libqcperf only for those three triples. Any other target stops in the build script.

### Linux ARM64 (`aarch64-unknown-linux-gnu`)

From the [libqcperf Linux ARM64 instructions](third_party/libqcperf/README.md#linux-arm64-compilation):

- [ARM GNU Toolchain](https://developer.arm.com/downloads/-/arm-gnu-toolchain-downloads) (`aarch64-none-linux-gnu-gcc`)
- `AARCH64_TOOLCHAIN_PATH` set to that toolchain's install directory
- `aarch64-none-linux-gnu-gcc` on `PATH` (the Rust linker for this target)
- For the NPU backend: Autotools (`autoconf`, `automake`, `libtool`) and `make`. The build initializes `third_party/libqcperf/qcperf/third-party/fastrpc` and the NPU backend needs `libcdsprpc.so` from that tree.

### Android ARM64 (`aarch64-linux-android`)

From the [libqcperf Android ARM64 instructions](third_party/libqcperf/README.md#android-arm64-compilation):

- [Android NDK](https://developer.android.com/ndk/downloads) r24 or newer
- `ANDROID_NDK_HOME` (or `ANDROID_NDK_ROOT`, or `NDK`) set to the NDK root
- `aarch64-linux-android29-clang` on `PATH` (NDK LLVM prebuilt `bin` directory; the host folder name depends on your OS)
- The same fastrpc / Autotools tools as Linux ARM64 when the NPU backend is included

On a device, if `libcdsprpc.so` is not found at runtime:

```bash
export LD_LIBRARY_PATH=/vendor/lib64
```

### Windows ARM64 (`aarch64-pc-windows-msvc`)

From the [libqcperf Windows ARM64 instructions](third_party/libqcperf/README.md#windows-arm64-compilation):

- Visual Studio 2026 with the MSVC ARM64 build tools
- CMake generator `Visual Studio 18 2026`, platform `ARM64`
- Git submodules initialized recursively

## Compilation

`cargo build` configures and compiles libqcperf from `third_party/libqcperf/qcperf`, then links every static library that CMake produced. `-DBACKENDS` is left empty, so libqcperf enables every backend that platform supports. Set `QCPERF_BACKENDS` only when you want a smaller build, for example `CPU;NPU`.

CMake output is under `target/libqcperf/<rust-target>/`. The TUI binary is under `target/<rust-target>/debug/` (or `release/` after `--release`).

### Linux ARM64

```bash
export AARCH64_TOOLCHAIN_PATH=/path/to/arm-gnu-toolchain
export PATH="$AARCH64_TOOLCHAIN_PATH/bin:$PATH"

cargo build --release --target aarch64-unknown-linux-gnu
```

Binary: `target/aarch64-unknown-linux-gnu/release/qcperf-tui`

### Android ARM64

```bash
export ANDROID_NDK_HOME=/path/to/android-ndk
export PATH="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/<host>/bin:$PATH"

cargo build --release --target aarch64-linux-android
```

`<host>` is the NDK prebuilt directory for the machine running the build, such as `linux-x86_64` or `windows-x86_64`.

Binary: `target/aarch64-linux-android/release/qcperf-tui`

Push it to the device and run it in a terminal there. The NPU backend still needs `libcdsprpc.so` on the loader path, usually `LD_LIBRARY_PATH=/vendor/lib64`.

### Windows ARM64

Use a Visual Studio 2026 developer shell that can see the ARM64 build tools, then:

```powershell
cargo build --release --target aarch64-pc-windows-msvc
```

Binary: `target\aarch64-pc-windows-msvc\release\qcperf-tui.exe`

### Narrow the backends

```bash
# Linux / Android
QCPERF_BACKENDS="CPU;NPU" cargo build --release --target aarch64-unknown-linux-gnu

# Windows
$env:QCPERF_BACKENDS = "CPU;THERMAL"
cargo build --release --target aarch64-pc-windows-msvc
```

Unset the variable and rebuild to return to every backend the platform supports. Names match libqcperf's `-DBACKENDS` list (`CPU`, `NPU`, `THERMAL`, `POWER`, and any later addition). A backend the platform does not support is skipped by libqcperf's own CMake.

Further detail on toolchains and the fastrpc submodule is in [third_party/libqcperf/README.md](third_party/libqcperf/README.md).
