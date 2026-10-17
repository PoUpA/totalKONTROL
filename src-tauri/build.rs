/// Files of the Windows MIDI Services API copied next to the executable.
const WINDOWS_MIDI_FILES: [&str; 2] = ["Windows.Devices.Midi2.dll", "Windows.Devices.Midi2.pri"];

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var_os("CARGO_FEATURE_WINDOWS_MIDI_SERVICES").is_some() {
            #[cfg(feature = "windows-midi-services")]
            windows_midi::generate();
        } else {
            // Left over by an earlier build with the feature. The MSI bundler
            // picks up every DLL next to the executable, and this one must not
            // ship without the Windows version check of the feature's installer.
            for file in WINDOWS_MIDI_FILES {
                let _ = std::fs::remove_file(profile_dir().join(file));
            }
        }
    }
    tauri_build::build()
}

/// target/<profile>, where the executable is built.
fn profile_dir() -> std::path::PathBuf {
    // OUT_DIR = target/<profile>/build/<crate>-<hash>/out
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    out_dir.ancestors().nth(3).unwrap().to_path_buf()
}

/// Generates the Rust bindings for Windows.Devices.Midi2 from the NuGet
/// package's .winmd, and copies the API DLL next to the executable
/// (registration-free activation).
#[cfg(feature = "windows-midi-services")]
mod windows_midi {
    use std::path::PathBuf;

    pub fn generate() {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let pkg = manifest.join("vendor/windows-midi2");
        let winmd = pkg.join("ref/native/Windows.Devices.Midi2.winmd");
        assert!(
            winmd.exists(),
            "{} introuvable : lancer d'abord scripts/fetch-windows-midi.ps1",
            winmd.display()
        );
        println!("cargo:rerun-if-changed={}", winmd.display());

        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let out = out_dir.join("midi2.rs");
        windows_bindgen::bindgen([
            "--in",
            "default",
            "--in",
            winmd.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--filter",
            "Windows.Devices.Midi2",
            "--reference",
            "windows,skip-root,Windows",
            "--no-comment",
        ])
        .unwrap();
        // include!() rejects inner attributes: strip the leading #![allow(...)],
        // which is put back on the module in winmidi.rs.
        let code = std::fs::read_to_string(&out).unwrap();
        let code = match code.strip_prefix("#![allow(") {
            Some(rest) => rest.split_once(")]").unwrap().1.to_string(),
            None => code,
        };
        std::fs::write(&out, fix_noexcept_statics(&code)).unwrap();

        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
            "aarch64" => "win-arm64",
            _ => "win-x64",
        };
        for file in super::WINDOWS_MIDI_FILES {
            let src = pkg.join("runtimes").join(arch).join("native").join(file);
            std::fs::copy(&src, super::profile_dir().join(file)).unwrap();
        }
    }

    /// windows-bindgen 0.66 generates broken code for static methods marked
    /// [noexcept] in the .winmd (MidiApi::EnsureServiceAvailable, etc.): the
    /// closure passed to the factory cache returns the raw value instead of a
    /// Result. Wrap the value in Ok(..) and unwrap the Result.
    fn fix_noexcept_statics(code: &str) -> String {
        let mut lines: Vec<String> = code.lines().map(str::to_string).collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim_start();
            if t.starts_with("Self::I") && t.ends_with("(|this| unsafe {") {
                let indent = lines[i][..lines[i].len() - t.len()].to_string();
                let close = format!("{indent}}})");
                let j = (i + 1..lines.len()).find(|&k| lines[k] == close).unwrap();
                if lines[i..j]
                    .iter()
                    .any(|l| l.contains("debug_assert!(hresult__.0 == 0)"))
                {
                    let last = lines[j - 1].clone();
                    let lt = last.trim_start();
                    let lindent = &last[..last.len() - lt.len()];
                    if lt.ends_with(';') {
                        lines.insert(j, format!("{lindent}Ok(())"));
                    } else {
                        lines[j - 1] = format!("{lindent}Ok({lt})");
                    }
                    let j = (i + 1..lines.len()).find(|&k| lines[k] == close).unwrap();
                    lines[j] = format!("{indent}}}).unwrap()");
                    i = j;
                }
            }
            i += 1;
        }
        lines.join("\n")
    }
}
