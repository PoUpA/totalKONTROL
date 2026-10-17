# Virtual MIDI device on Windows

On Windows, `midir` cannot create virtual ports. Without help, the app needs two loopMIDI ports to talk to the DAW. With the **Windows MIDI Services** (WinRT API `Windows.Devices.Midi2`, "app-to-app" virtual devices), the app creates its own device instead. This code lives in [src-tauri/src/winmidi.rs](../src-tauri/src/winmidi.rs) and is compiled only with the `windows-midi-services` Cargo feature.

## What was verified

A prototype validated the approach on Windows 11 25H2 (build 26200.9457):

- the app creates the virtual device `totalKONTROL`: one bidirectional function block on group 1;
- Windows exposes it, after about 200 ms, as a WinMM input and output named exactly `totalKONTROL`, visible to `midir` and to the DAW;
- app → DAW: notes sent by the device (UMP type 2) reach WinMM as MIDI 1.0 bytes (`90 3C 64`, …);
- DAW → app: what the DAW sends to the `totalKONTROL` output reaches the device (`2090307F` = `90 30 7F` on group 1).

A single device therefore replaces the loopMIDI pair `totalKONTROL` and `totalKONTROL feedback`.

## Known Windows bug

The MIDI service shipped with Windows on the test PC (`MidiSrv.exe` 10.0.26100.8875, `Midi2.VirtualMidiTransport.dll` 1.0.15) has a known bug, [microsoft/MIDI#1047](https://github.com/microsoft/MIDI/issues/1047) (duplicates: #1236, #651). After each service start, only the **first** session can create a virtual device. Closing it (`DisconnectEndpointConnection` / `MidiSession::Close`) never returns, and every later creation hangs.

Microsoft has fixed it. The fix ships in Windows 11 25H2 and later with the optional update at the end of November 2026 ("11D"), then rolls out gradually: up to about 45 days on an up-to-date PC, so around mid-January 2027.

In practice the first launch of the app after the service starts works, but the next launch (including every automatic restart of `npm run dev:winmidi`) freezes the whole Windows MIDI stack: WinMM calls hang for every app, the DAW included, and no MIDI port shows up anywhere.

How the app copes:

- it reads the version of `Midi2.VirtualMidiTransport.dll` in `System32`. Up to 1.0.15 (the buggy version), it does not offer the virtual port and explains why; it offers it again once Windows ships a newer transport;
- with a fixed transport, it creates the device once, on first use, and never closes it: the device goes away with the process;
- device creation, port listing and connection all have a timeout. When the MIDI service hangs, the app says so instead of freezing or showing empty lists;
- to unblock the service, close the app, the DAW and any MIDI app, then run [scripts/restart-midisrv.ps1](../scripts/restart-midisrv.ps1) in an administrator PowerShell. It kills `midisrv.exe` and starts the service again.

## End-user requirements

| Item                                     | Status                                                     |
| ---------------------------------------- | ---------------------------------------------------------- |
| Windows 11 25H2 or later (build ≥ 26200) | required (Microsoft does not support 24H2)                 |
| `midisrv` service and virtual transport  | already part of Windows                                    |
| `Windows.Devices.Midi2.dll` API          | **not in Windows yet**: ships next to the app's executable |
| Fix for bug #1047                        | Windows update at the end of November 2026                 |
| loopMIDI                                 | no longer needed                                           |

The API (`Windows.Devices.Midi2.dll` + `.pri`, about 7 MB for x64) comes from the `Windows.Devices.Midi2` NuGet package, published on the [GitHub releases](https://github.com/microsoft/MIDI/releases). The version used is `0.99.92-preview.11` ("API surface lock"). Its "go live" license allows shipping it with the app, provided that:

- it is not installed on Windows versions older than Windows 11 25H2;
- it is removed once the API is part of Windows;
- the app is rebuilt against the Windows `.winmd` within 90 days of its release.

Once Windows has its own copy, Windows uses it even if the app ships one. `MidiApi::IsSystemProvided()` tells which one is active.

If the API is missing, or the service or transport is unavailable, the UI does not offer the virtual port and the app falls back to loopMIDI.

## Building

1. Run [scripts/fetch-windows-midi.ps1](../scripts/fetch-windows-midi.ps1). It downloads `Windows.Devices.Midi2.0.99.92-preview.11.nupkg` (103 MB) from the [inbox-preview-11](https://github.com/microsoft/MIDI/releases/tag/inbox-preview-11) release and extracts it into `src-tauri/vendor/windows-midi2/` (ignored by git).
2. Build with the feature: `npm run dev:winmidi`, or `cargo build --features windows-midi-services` in `src-tauri`.

At build time, [src-tauri/build.rs](../src-tauri/build.rs) generates the Rust bindings from the package's `.winmd` with `windows-bindgen`, and copies the DLL and `.pri` next to the executable. `windows-rs` then activates the API without registration, from the executable's folder.

## Installers

`npm run build:winmidi` builds the `.msi` and NSIS installers with the feature. It runs `tauri build --features windows-midi-services` with the extra config file [src-tauri/tauri.windows-midi.conf.json](../src-tauri/tauri.windows-midi.conf.json), merged over `tauri.conf.json`, so the plain build (`npm run tauri build`) needs neither the package nor the DLL. The extra config:

- adds `Windows.Devices.Midi2.dll` and `Windows.Devices.Midi2.pri` to `bundle.resources`, in the map form (`"source": "target"`) so that they land at the root of the resource folder, which on Windows is the installation folder: next to `totalkontrol.exe` (checked in the generated NSIS script and in the `.msi` file table);
- limits the bundle targets to `msi` and `nsis`;
- adds the Windows version check below.

The MSI bundler also picks up every DLL found next to the executable in `target/release`. To keep a plain build made after a build with the feature from shipping the DLL without that check, `build.rs` deletes the copies of `Windows.Devices.Midi2.dll` and `.pri` when the feature is off.

The [release workflow](../.github/workflows/release.yml) builds this variant in its own Windows job, after downloading the package with `scripts/fetch-windows-midi.ps1`, and adds `_winmidi` to the installer names.

### Windows 11 25H2 minimum

The DLL's "go live" license forbids installing it before Windows 11 25H2. Rather than installing the app without the DLL on older systems, both installers refuse to install there, and say that the plain variant (with loopMIDI) is the one to use:

- NSIS: [src-tauri/windows/windows-midi-hooks.nsh](../src-tauri/windows/windows-midi-hooks.nsh) checks the build number with `${AtLeastBuild} 26200` (`WinVer.nsh`, which reads the real build, not the one the manifest lets Windows report). The check runs when the installer window opens, before any page, and again in `NSIS_HOOK_PREINSTALL`, before any file is copied, for silent installs (`/S`) that skip the window.
- MSI: [src-tauri/windows/windows-midi-fragment.wxs](../src-tauri/windows/windows-midi-fragment.wxs) reads `CurrentBuildNumber` under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` (the MSI properties `VersionNT` and `WindowsBuild` stop at Windows 8.1) and adds a launch condition `Installed OR build >= 26200`. Uninstalling stays possible.

The app itself does not check the Windows version: it only runs where the installer agreed to install it, and on a Windows without a fixed MIDI service it falls back to loopMIDI.

## API used

Namespace `Windows.Devices.Midi2` (WinRT):

1. `MidiApi::EnsureServiceAvailable()`, `MidiVirtualDeviceManager::IsTransportAvailable()`
2. `MidiSession::Create(name)`
3. `MidiDeclaredEndpointInfo` (name, product instance id, MIDI 1.0 protocol) + `MidiFunctionBlock` (number 0, group 0, 1 group, `Bidirectional`, `RepresentsMidi10Connection = YesBandwidthUnrestricted`)
4. `MidiVirtualDeviceCreationConfig::CreateInstance(name, description, manufacturer, info)`, then `FunctionBlocks().Append(block)`
5. `MidiVirtualDeviceManager::CreateVirtualDevice(config)`, `SetSuppressHandledMessages(true)`
6. `session.CreateEndpointConnection(device.DeviceEndpointDeviceId())`, `MessageReceived(...)`, **`AddMessageProcessingPlugin(device)`** (without it the device never receives anything), `Open()`
7. Sending: `SendSingleMessageWords(0, 0x2G9n_kkvv)` (UMP type 2 = MIDI 1.0 voice)
8. Receiving: `PeekFirstWord()` in `MessageReceived`

What the DAW sees: Windows creates a second, client-side endpoint (`MidiVirtualDeviceManager::GetAssociatedClientEndpointDeviceId`) and its WinMM ports. Their numbers are assigned asynchronously: they appear a few hundred milliseconds after the device opens.

Points to keep in mind:

- WinMM port names are at most 32 characters, terminator included.
- `windows-bindgen` 0.66 generates static methods marked `[noexcept]` incorrectly. `build.rs` patches the generated code (`fix_noexcept_statics`).
- See also Microsoft's guide [Porting a MIDI library](https://microsoft.github.io/MIDI/kb/porting-midi-libraries/).
