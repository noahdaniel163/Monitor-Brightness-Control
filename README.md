# Monitor Brightness Control

A lightweight Windows 10/11 desktop application for controlling monitor brightness and contrast through DDC/CI.

Copyright (c) 2026 noahdaniels

## Features

- Automatic monitor detection through Windows DDC/CI.
- Independent controls for every connected monitor.
- Brightness control using VCP `0x10`.
- Contrast control using VCP `0x12`.
- Reads the current monitor values and VCP limits.
- Applies changes while dragging the sliders.
- Background DDC/CI worker thread so the UI stays responsive.
- Automatic monitor refresh and manual `Refresh monitors` action.
- Dark, minimal English interface.
- Embedded `icon.ico` for the application window and Windows executable metadata.
- Release executable built as a Windows GUI application without a console window.

## Requirements

- Windows 10 or Windows 11.
- Rust stable toolchain for building from source.
- A monitor and connection path that support DDC/CI.
- DDC/CI enabled in the monitor's on-screen menu.

Some laptop panels, docks, adapters and KVM switches do not expose DDC/CI. The application does not use software dimming as a fallback.

## Build from source

```powershell
cd "E:\Script\rust\monitor brightness control"
cargo build --release
```

The executable is generated at:

```text
target\release\monitor-brightness-control.exe
```

For development:

```powershell
cargo run
```

## Usage

1. Start `monitor-brightness-control.exe`.
2. Wait for the monitor scan to finish.
3. Drag the `Brightness` or `Contrast` slider for the desired monitor.
4. Use `Refresh monitors` after connecting or disconnecting a display.
5. Minimize or close the window when you are done using it.

## DDC/CI troubleshooting

If a monitor is shown as unavailable:

- Enable DDC/CI in the monitor's OSD/settings menu.
- Connect the monitor directly to the GPU when possible.
- Avoid docks, KVMs and adapters that block DDC/CI.
- Try a different HDMI, DisplayPort or USB-C cable.
- Confirm that the monitor supports VCP `0x10` and `0x12`.
- Use `Refresh monitors` after changing the connection.

The application reports DDC/CI errors instead of changing another monitor or using an overlay-based brightness workaround.

## Project structure

```text
src/
  main.rs          # egui/eframe UI and application state
  ddc_worker.rs    # background monitor discovery and DDC/CI commands
  icon.ico         # application icon
build.rs           # embeds Windows icon and executable metadata
Cargo.toml         # Rust dependencies and release settings
```

## Main dependencies

- [eframe](https://crates.io/crates/eframe) / [egui](https://crates.io/crates/egui) for the GUI.
- [ddc-hi](https://crates.io/crates/ddc-hi) for DDC/CI monitor communication.
- [image](https://crates.io/crates/image) for loading the ICO resource.
- [winres](https://crates.io/crates/winres) for embedding Windows executable metadata.

## Validation

```powershell
cargo fmt --all
cargo check --release
cargo build --release
```

## License

Copyright (c) 2026 noahdaniels. All rights reserved unless a separate license is added to this repository.
