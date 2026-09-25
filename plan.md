# qcperf-tui implementation plan

Rust terminal UI that links **libqcperf** from the `third_party/libqcperf` submodule, shows live metric graphs, and lets several dashboards profile at the same time.

This plan interprets `Requirement.md` as follows:

- On startup the TUI calls `qcperf_init`, connects each backend compiled into the library, and reads capabilities with `qcperf_get_capabilities_info`.
- A built-in dashboard shows CPU use and other common metrics that those capabilities actually expose.
- The user can create more dashboards, pick metrics, and pick how each metric is drawn.
- Percentage metrics default to a pie (current share). Frequency and other units (MHz and similar) default to a time-series line or bar, in the style of Windows Task Manager: a rolling window of recent samples, not a single number.
- Each capability’s sampling rate and streaming rate are chosen from the discrete millisecond lists libqcperf returns on `QcPerfCapabilityInfo`.
- Every dashboard has Edit, Start, Stop, Snapshot, and Save CSV.
- Leaving a dashboard does not stop it. Two or more dashboards may profile together.
- The layout refits when the terminal is resized.
- Colors have defaults and can be changed per series.
- The binary must build for ARM GNU Linux, Android (NDK), and Windows ARM64 (MSVC).
- A backend added or enabled inside libqcperf shows up in the TUI with no new Rust match arm, dashboard, or chart code. The TUI only understands the public capability and metric records.

There is no mock backend and no `Profiler` trait. Startup, streaming, and shutdown call libqcperf directly through a thin FFI module. Host-side tests cover only pure Rust (rate checks, chart defaults, ring buffers, CSV). Live profiling is verified by running the binary on the target device.

## libqcperf contract

Public headers under the submodule:

- `third_party/libqcperf/qcperf/core/inc/qcperf.h`
- `third_party/libqcperf/qcperf/core/inc/qcperf_common.h`
- `third_party/libqcperf/qcperf/backends/inc/qcperf_backend_enum.h`

Call order:

1. `qcperf_init`
2. `qcperf_connect_backend(backend_id, message_callback)` for each backend that was compiled in
3. `qcperf_get_capabilities_info(backend_id, &info)`
4. `qcperf_set_data_callback(backend_id, data_callback)` once per connected backend
5. `qcperf_start(backend_id, &QcPerfRequest)` for each capability a running dashboard needs
6. On stop: `qcperf_stop` with the same request
7. On quit: stop every request, `qcperf_disconnect_backend` for each connection, then `qcperf_deinit`

`QcPerfRequest` is one capability, not one metric:

```c
struct QcPerfRequest {
    uint8_t capability_id;
    uint16_t streaming_rate; /* milliseconds, must be one of capability.streaming_rate[] */
    uint16_t sampling_rate;  /* milliseconds, must be one of capability.sampling_rate[] */
};
```

`qcperf_start` delivers every metric of that capability through the backend data callback (`QcPerfData`: `backend_id`, `capabilityId`, array of `QcPerfMetricResponse`). The callback runs on a libqcperf thread. The FFI callback copies values into an owned queue and returns. It does not call back into libqcperf.

A capability has one active request. Dashboards that show metrics from the same `(backend_id, capability_id)` share that request. Sampling and streaming rates are chosen once per capability, from that capability’s lists, and apply to every graph fed by it. The editor rejects a second rate on the same capability while it is already started. Changing the rate means stopping every dashboard subscribed to that capability, then starting again.

## New backends need no TUI edits

libqcperf already treats backends as data. `qcperf/cmake/BuildConfig.cmake` enables every platform-supported backend when `-DBACKENDS` is left empty, and `QC_PERF_BACKEND_MAX` is the sentinel after the last id in `qcperf_backend_enum.h`. A backend that is not compiled in returns `QC_PERF_RETURN_CODE_INVALID_BACKEND_ID` from `qcperf_connect_backend`.

The TUI follows that. It does not list backend names, ids, or metric schemas in Rust.

- `build.rs` does not pass `-DBACKENDS`. libqcperf then compiles every backend that platform supports, including ones added later in `BuildConfig.cmake`. Set the environment variable `QCPERF_BACKENDS` (for example `CPU;NPU`) only to narrow a local build. That variable is forwarded as `-DBACKENDS` and is not read by UI code.
- `QcPerf::init` loops `0..QC_PERF_BACKEND_MAX` from the binding generated out of the submodule header. It connects each id, skips `INVALID_BACKEND_ID`, and copies `QcPerfBackendInfo`. There is no `match` on `QC_PERF_BACKEND_QCOM_LINUX_CPU` or any other variant.
- `build.rs` runs bindgen on every build against the submodule headers, so a new enumerator inserted before `QC_PERF_BACKEND_MAX` is picked up by the next compile. No checked-in backend table.
- The editor, default dashboard, pies, lines, and bars read only `capability_name`, `metric_name`, `metric_unit`, and the sampling and streaming arrays. They never branch on which backend produced the record.
- Labels in the UI are the capability name plus the numeric `backend_id`. There is no Rust map from id to "CPU" or "NPU". When libqcperf adds a name field on `QcPerfBackendInfo`, the wrapper copies it; until then the capability name is the label.

Adding a backend inside libqcperf (enum slot, `backend_init_fns` entry, `BuildConfig.cmake` platform list) and rebuilding this repo is enough. Rust source changes only if `qcperf.h` itself changes shape: new request fields, a new callback signature, or a new value type in `QcPerfGenericType`.

## Targets

libqcperf’s own toolchains (from its README) drive the C build. The Rust binary uses the matching target triple and links the static library (`-DBUILD_SHARED=OFF`), with `QCPERF_STATIC_LIBRARY` defined so the headers do not `dllimport`.

| Requirement | Rust target | C toolchain |
|---|---|---|
| ARM GNU compiler | `aarch64-unknown-linux-gnu` | `aarch64-none-linux-gnu-gcc` via `AARCH64_TOOLCHAIN_PATH`, `-DTARGET_ARCH=linux-aarch64` |
| Android compiler | `aarch64-linux-android` | NDK r24+, `android.toolchain.cmake`, `arm64-v8a`, API 29 |
| Windows Microsoft ARM64 | `aarch64-pc-windows-msvc` | Visual Studio 2022, `-G "Visual Studio 17 2022" -A ARM64` |

`-DBACKENDS` is omitted, so each target links every backend libqcperf supports on that OS. `QCPERF_BACKENDS` overrides that for one build.

NPU needs the fastrpc submodule (`libcdsprpc`). `build.rs` runs `git submodule update --init` inside `third_party/libqcperf` before the Android and Linux configures. On device, if `libcdsprpc.so` is not next to the process, `LD_LIBRARY_PATH` must include `/vendor/lib64`.

## Stack

- Language: Rust 2021, edition pinned in `rust-toolchain.toml` (stable).
- TUI: `ratatui` + `crossterm`. Both work on Windows consoles and on Linux/Android PTYs. No GUI toolkit.
- FFI: `bindgen` runs from `build.rs` on the submodule headers every build, so `QC_PERF_BACKEND_MAX` and the structs stay aligned with libqcperf. Hand-written wrappers in `src/backend/qcperf.rs` never name a backend.
- Config and dashboard files: `serde` + `toml`.
- IDs: `uuid`.
- Errors: `thiserror` at the Rust boundary. libqcperf failures become `QcPerfError { code, message }` using `qcperf_get_error_info`.
- Concurrency: `std::thread` and `std::sync` channels. No async runtime.

Do not pull Unix-only crates (`nix`, unguarded `libc` calls, signal handlers) into the dependency graph.

## Crate layout

```
qcperf-tui/
  Cargo.toml
  rust-toolchain.toml
  .cargo/config.toml              # linkers for the three targets
  build.rs                        # cmake the submodule, emit rustc-link-*
  plan.md
  Requirement.md
  third_party/libqcperf/          # git submodule
  config/
    colors.toml
    dashboards/                   # persisted dashboards (created at runtime)
  src/
    main.rs
    error.rs
    app.rs
    model/
      mod.rs
      unit.rs
      capability.rs               # owned copy of QcPerfBackendInfo
      graph.rs
      dashboard.rs
      sample.rs
    backend/
      mod.rs
      bindings.rs                 # bindgen output
      qcperf.rs                   # init, connect, start, stop, callback bridge
      session.rs                  # capability sessions shared by dashboards
    ui/
      mod.rs
      list.rs
      view.rs
      editor.rs
      help.rs
      widgets/
        pie.rs
        line_chart.rs
        bar_chart.rs
    export/
      csv.rs
      snapshot.rs
    theme.rs
    persist.rs
  tests/
    model_rates.rs
    csv_export.rs
```

## Build of the submodule

`build.rs` configures and builds `third_party/libqcperf/qcperf` for `env::var("TARGET")` when the static library is missing or older than the submodule sources:

| `TARGET` | CMake | Artifact |
|---|---|---|
| `aarch64-unknown-linux-gnu` | `-DTARGET_ARCH=linux-aarch64 -DBUILD_SHARED=OFF` | `libqcperfCore.a` |
| `aarch64-linux-android` | NDK toolchain, `-DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-29 -DBUILD_SHARED=OFF` | `libqcperfCore.a` |
| `aarch64-pc-windows-msvc` | `-G "Visual Studio 17 2022" -A ARM64` | `qcperfCore.lib` |

If `QCPERF_BACKENDS` is set, append `-DBACKENDS=<that value>`. Otherwise leave the cache variable empty so libqcperf enables its full platform list.

Build directory: `target/libqcperf/<target>/`. `build.rs` prints `cargo:rustc-link-search` and `cargo:rustc-link-lib=static=qcperfCore`, plus `cargo:rustc-cfg=qcperf_static` so the Rust crate defines `QCPERF_STATIC_LIBRARY` for the C headers included by bindgen. It also reruns when files under `third_party/libqcperf/qcperf` change.

`.cargo/config.toml` sets the Rust linker to the same compiler CMake uses (`aarch64-none-linux-gnu-gcc`, NDK `aarch64-linux-android29-clang`). The Windows ARM64 target uses the default MSVC linker.

Other host triples fail the build with a message that names the three supported targets. There is no fallback library.

## Domain model

```rust
pub struct Capability {
    pub backend_id: u8,
    pub capability_id: u8,
    pub name: String,
    pub metrics: Vec<MetricInfo>,
    pub sampling_rates_ms: Vec<u16>,   // copied from sampling_rate[0..sampling_rate_len]
    pub streaming_rates_ms: Vec<u16>,
}

pub struct MetricInfo {
    pub metric_id: u16,
    pub name: String,
    pub description: String,
    pub unit: String,                  // raw metric_unit, e.g. "%", "MHz", "°C"
}

pub enum ChartKind {
    Pie,    // default when unit is "%" or contains "percent"
    Line,   // default for every other unit
    Bar,
}

pub struct GraphSpec {
    pub backend_id: u8,
    pub capability_id: u8,
    pub metric_id: u16,
    pub chart: ChartKind,
    pub color: SeriesColor,
    pub window: Duration,              // visible history, default 60s
}

/// One per (backend, capability) on a dashboard. Rates are not per graph.
pub struct CapabilityRate {
    pub backend_id: u8,
    pub capability_id: u8,
    pub sampling_rate_ms: u16,
    pub streaming_rate_ms: u16,
}

pub struct Dashboard {
    pub id: DashboardId,
    pub name: String,
    pub graphs: Vec<GraphSpec>,
    pub rates: Vec<CapabilityRate>,
    pub run: RunState,                 // Idle | Running | Stopped
}

pub struct Sample {
    pub at: SystemTime,
    pub backend_id: u8,
    pub capability_id: u8,
    pub metric_id: u16,
    pub value: f64,                    // numeric types only; strings are skipped for charts
}
```

Rules in `model` (no FFI):

- `CapabilityRate::validate(&Capability)` requires both rates to be members of that capability’s lists. Streaming shorter than sampling is rejected even if both values appear in the lists, matching the library’s sample-then-stream behavior (`samples_per_stream = ceil(streaming / sampling)`).
- `ChartKind::default_for(unit)` returns `Pie` when the unit is `%` or the word percent (any case), and `Line` otherwise. The editor can still switch a series to `Bar` or `Line`.
- Ring capacity is `window / streaming_period`, capped at 4096 points per series.

`backend/qcperf.rs` deep-copies `QcPerfBackendInfo` into these owned structs before the library reuses the buffers. String fields respect `*_len`.

## Sessions shared across dashboards

```rust
struct SessionTable {
    /// key: (backend_id, capability_id, sampling_rate_ms, streaming_rate_ms)
    active: HashMap<SessionKey, Subscribers>,
}

struct Subscribers {
    dashboards: Vec<DashboardId>,
    rings: HashMap<(DashboardId, u16 /* metric_id */), SampleRing>,
}
```

1. **Start** on a dashboard calls `qcperf_start` only for session keys that are not already active. Metrics from that callback are appended to every subscriber’s ring for the metric ids that dashboard selected. Metrics the dashboard did not select are ignored.
2. **Stop** removes that dashboard. `qcperf_stop` runs only when the subscriber list for that key is empty.
3. Switching the visible dashboard, opening the editor, or saving CSV does not call stop.
4. Quit stops every key, disconnects every backend, then `qcperf_deinit`.
5. The data callback sends `AppEvent::Samples` on a channel. The UI thread only draws. Message callbacks (`QcPerfMessage`) with level error or warning are shown in the dashboard status line.

Two dashboards that pick the same capability and the same rates share one `qcperf_start`. A dashboard that needs a different rate for a capability that is already started is told which rate is live and is not given a second request.

## UI flow

```
startup
  qcperf_init
  for id in 0..QC_PERF_BACKEND_MAX: connect, skip INVALID_BACKEND_ID, copy capabilities
  load colors.toml and dashboards/*.toml
  if no saved dashboards: materialize "Overview" from every connected capability
  qcperf_set_data_callback on each connected backend
  enter Dashboard List

Dashboard List
  Up/Down select, Enter open, n new, d delete (stopped only), q quit

Live view
  header: name, Running/Stopped, capability rates
  body: responsive grid of graphs
  e edit
  s start, x stop
  p snapshot, c export CSV
  Tab or Left/Right cycle dashboards without stopping
  Esc back to list, q quit

Editor
  capabilities grouped by backend (name, metrics, unit, legal rates)
  space toggles a metric onto this dashboard
  chart kind, color, window per metric
  sampling and streaming rate once per capability, picked from the two lists
  Enter save, Esc cancel
```

Color-only and window-only edits apply immediately to a running dashboard. A rate change on a live capability is saved but applied only after Stop and Start, and only if no other running dashboard still holds the old session.

### Scalable grid

`ratatui::layout::Layout` is recomputed every frame from `terminal.size()`:

| Terminal width | Columns |
|---|---|
| < 60 | 1 |
| 60–119 | 2 |
| >= 120 | 3 |

Each cell shows the metric name, the latest value with its unit string, and the chart. A pie is the current value. When several percent metrics from one capability are selected together (for example per-core CPU), they share one pie with one slice each. Line and bar charts plot that metric’s ring across `window`. The axis label shows the window (for example `60s`) and the unit. Resizing redraws on the next frame.

### Charts

- **Pie** (`ui/widgets/pie.rs`): custom `Widget`, Unicode block sectors, legend with name and value.
- **Line** (`ui/widgets/line_chart.rs`): `ratatui::widgets::Chart` over the ring. Y is `0..=100` when the unit is percent; otherwise padded min/max of the window. X is time.
- **Bar** (`ui/widgets/bar_chart.rs`): `BarChart` of the last N buckets in the window. Bucket count follows the cell width.

Default series colors come from `theme.rs`. The editor writes an override into `GraphSpec.color`. `colors.toml` replaces the palette.

### Default dashboard

If the user has no saved file, build a dashboard named `Overview` from every capability that connected, whichever backends those are:

- metrics whose unit is `%` or contains `percent`, as pie slices grouped by capability
- metrics whose unit is `MHz` or `Hz`, as 60s line charts
- any other numeric metric, as a line chart with `ChartKind::default_for`

Nothing in this factory compares `backend_id` to a CPU, NPU, thermal, or power constant. A backend added in libqcperf contributes its metrics on the next launch.

Default rates are the first entry in each capability’s `sampling_rate` and `streaming_rate` arrays, with streaming raised to the smallest listed value that is greater than or equal to the sampling rate.

## CSV and snapshot

**CSV** (`export/csv.rs`), one file per dashboard:

```
timestamp_rfc3339,dashboard,backend_id,capability_id,metric_id,metric_name,value,unit
```

Path: `config/exports/<dashboard-name>-<utc-date>.csv`. The `c` key writes the current rings. While a dashboard is running and export is enabled, each sample batch is appended. The write clones the rings first so the callback thread is not blocked on disk.

**Snapshot** (`export/snapshot.rs`): `config/snapshots/<dashboard>-<utc-timestamp>.json` with graph specs, rates, and current rings. `RunState` does not change. A running dashboard keeps its `qcperf_start` after the snapshot.

## Persistence

- `config/colors.toml` — palette.
- `config/dashboards/<id>.toml` — name, graphs, capability rates, colors, window. `RunState` is not restored as Running. Every launch stays Idle until Start.
- Paths resolve next to the executable, overridable with `QCPERF_TUI_CONFIG`.

## FFI wrapper

`src/backend/qcperf.rs` exposes:

```rust
pub struct QcPerf { /* connected backends, capability tables */ }

impl QcPerf {
    pub fn init() -> Result<Self>;
    pub fn capabilities(&self) -> &[Capability];
    pub fn start(&mut self, rate: &CapabilityRate) -> Result<()>;
    pub fn stop(&mut self, rate: &CapabilityRate) -> Result<()>;
    pub fn shutdown(self) -> Result<()>;
}
```

`start` / `stop` build a `QcPerfRequest` and call the C functions. The data callback parses `QcPerfGenericType` (`bool`, `uint64`, `int64`, `double` become `f64`; strings are dropped for charts) and uses `timestamp` from `QcPerfMetricResponse` when it is non-zero, otherwise `SystemTime::now()`.

Init failure is drawn on the list screen with the string from `qcperf_get_error_info`. The process still reaches `qcperf_deinit` on quit if init succeeded.

## Implementation steps

### 1. Workspace skeleton and link

- Add `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `src/main.rs`, `src/error.rs`.
- Dependencies: `ratatui`, `crossterm`, `serde`, `toml`, `uuid`, `thiserror`. Build-dependency: `bindgen` only if bindings are regenerated; the checked-in `bindings.rs` is what the crate compiles.
- `build.rs` builds the submodule static library for the active target and emits link lines.
- `main` calls `QcPerf::init`, prints connected backends and capability names, then `shutdown`. No UI yet.

### 2. Model and validation

- Implement `unit` helpers, `capability`, `graph`, `dashboard`, `sample`.
- `SampleRing` with push, window trim, and CSV rows.
- Tests: rate not in the capability list, streaming shorter than sampling, pie-vs-line default from `%` and `MHz`, ring trim at the window edge.

### 3. FFI session layer

- Bindings for the three headers with include paths into the submodule and `-DQCPERF_STATIC_LIBRARY`.
- `QcPerf::init` connects `0..QC_PERF_BACKEND_MAX`, skips `INVALID_BACKEND_ID`, and copies capabilities. No backend id is named in this file.
- Register one data callback and one message callback per connected backend.
- `SessionTable` reference-counts `qcperf_start` / `qcperf_stop`.
- Callback copies samples onto the channel and returns `QC_PERF_RETURN_CODE_SUCCESS`.

### 4. App shell

- `App` holds dashboards, `QcPerf`, session table, theme, and `Screen`.
- Event loop merges crossterm input with the sample channel and draws at most every 50 ms or on input.
- Screens: dashboard list and a live view that shows metric names and the latest number.
- Keys: navigation, start, stop, switch dashboard, quit with stop-all, disconnect, deinit.

### 5. Chart widgets and responsive grid

- Pie, line, and bar widgets fed by `SampleRing`.
- Grid column rule from terminal width.
- Default `Overview` dashboard factory from capability units across every connected backend.
- On device: resize the terminal and confirm cells reflow; let a line chart run longer than its window and confirm it scrolls.

### 6. Editor

- Capability list from the live `QcPerf` tables.
- Multi-select metrics, chart kind, color cycle, window presets (15 s, 30 s, 60 s, 5 min).
- Sampling and streaming controls step through `sampling_rates_ms` and `streaming_rates_ms` only.
- Save writes the dashboard. Start opens or joins sessions. A rate edit on a shared live capability is refused until every subscriber has stopped.

### 7. CSV, snapshot, theme file

- Append CSV on each sample batch when export is enabled, and on the `c` key.
- `p` writes a JSON snapshot and does not call `qcperf_stop`.
- Load and save `colors.toml` and `dashboards/*.toml`.
- Tests: CSV header and one data row from an in-memory ring. Snapshot content is built without calling libqcperf.

### 8. Cross builds

- Document the three cmake and `cargo build --target` commands in the README.
- Produce:

```
cargo build --target aarch64-unknown-linux-gnu
cargo build --target aarch64-linux-android
cargo build --target aarch64-pc-windows-msvc
```

- Each command must configure libqcperf from the submodule and link `qcperfCore`. `cargo test` on the host covers the pure Rust tests and does not need the aarch64 library.

## Acceptance checklist

- Launch calls `qcperf_init`, connects every id below `QC_PERF_BACKEND_MAX` that the linked library accepts, and lists those capabilities (or shows `qcperf_get_error_info` text).
- Enabling another backend in libqcperf and rebuilding does not require a Rust change. The new capability appears in the editor and on `Overview`.
- Default dashboard Starts and shows percent metrics as pies and MHz metrics as scrolling lines, from every connected backend.
- A second dashboard can Start while the first keeps updating after focus moves.
- Two dashboards on the same capability and the same rates produce one `qcperf_start`.
- Stop affects only the selected dashboard and calls `qcperf_stop` only when it was the last subscriber.
- The editor offers only sampling and streaming values from that capability’s arrays.
- Snapshot and CSV files contain the selected metrics and do not stop streaming.
- Colors change from the editor and from `colors.toml`.
- Narrow and wide terminals both show every graph, reflowed.
- `cargo test` passes.
- Release builds exist for `aarch64-unknown-linux-gnu`, `aarch64-linux-android`, and `aarch64-pc-windows-msvc`, each linked to libqcperf built from `third_party/libqcperf`.

## Out of scope

- A fake profiler inside this crate, or a Rust module per libqcperf backend.
- Trace exploration beyond metric streams.
- A graphical desktop window. Output is the terminal.
- Persisting a dashboard as already Running across process restarts.
- Changes to libqcperf itself. The submodule is consumed as-is.
