use std::{fs, io, path::Path};

use romm_sync_core::FileMapping;
use romm_sync_planner::{CompatibilityStatus, Platform, SaveModel};

use crate::{
    AdapterDefinition, AdapterError, AdapterResolution, GBA_MGBA, NDS_MELONDS_DS, NES_MESEN,
    ReadinessReason, SNES_SNES9X,
    resolution::{MemberFormat, safe_file_name},
};

/// Accepted battery-save sizes for a platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SizeRule {
    Exact(&'static [u64]),
    PowerOfTwo { min: u64, max: u64 },
}

impl SizeRule {
    #[must_use]
    pub fn accepts(self, size: u64) -> bool {
        match self {
            Self::Exact(sizes) => sizes.contains(&size),
            Self::PowerOfTwo { min, max } => size.is_power_of_two() && (min..=max).contains(&size),
        }
    }
}

/// A `RetroArch` per-content battery-save configuration.
///
/// `RetroArch` writes `<content-basename>.srm` into the effective save root.
/// The audited Beelink configuration disables core and content sorting, so the
/// root is the global save directory rather than a core subdirectory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryProfile {
    pub definition: &'static AdapterDefinition,
    pub local_extension: &'static str,
    pub upload_extension: &'static str,
    pub remote_extensions: &'static [&'static str],
    /// Known same-stem files that belong to the save but cannot be represented
    /// by the one-file `RomM` 5.4 transfer boundary.
    pub companion_extensions: &'static [&'static str],
    /// Same-stem files that discovery must ignore, such as the manual DS `.sav`
    /// test artifact recorded during M0.
    pub excluded_extensions: &'static [&'static str],
    pub sizes: SizeRule,
    pub romm_emulator: &'static str,
}

pub const NDS_BATTERY: BatteryProfile = BatteryProfile {
    definition: &NDS_MELONDS_DS,
    local_extension: "srm",
    upload_extension: "sav",
    remote_extensions: &["sav", "srm"],
    companion_extensions: &["rtc"],
    excluded_extensions: &["sav"],
    sizes: SizeRule::PowerOfTwo {
        min: 512,
        max: 32 * 1024 * 1024,
    },
    romm_emulator: "melondsds",
};

/// GBA backup media: 512 B / 8 KiB EEPROM, 32 KiB SRAM, 64/128 KiB Flash.
pub const GBA_BATTERY: BatteryProfile = BatteryProfile {
    definition: &GBA_MGBA,
    local_extension: "srm",
    upload_extension: "sav",
    remote_extensions: &["sav", "srm"],
    companion_extensions: &["rtc"],
    excluded_extensions: &[],
    sizes: SizeRule::Exact(&[512, 8192, 32768, 65536, 131_072]),
    romm_emulator: "mgba",
};

pub const NES_BATTERY: BatteryProfile = BatteryProfile {
    definition: &NES_MESEN,
    local_extension: "srm",
    upload_extension: "sav",
    remote_extensions: &["sav", "srm"],
    companion_extensions: &[],
    excluded_extensions: &[],
    sizes: SizeRule::PowerOfTwo {
        min: 128,
        max: 1024 * 1024,
    },
    romm_emulator: "mesen",
};

pub const SNES_BATTERY: BatteryProfile = BatteryProfile {
    definition: &SNES_SNES9X,
    local_extension: "srm",
    upload_extension: "sav",
    remote_extensions: &["sav", "srm"],
    companion_extensions: &["rtc"],
    excluded_extensions: &[],
    sizes: SizeRule::PowerOfTwo {
        min: 2048,
        max: 512 * 1024,
    },
    romm_emulator: "snes9x",
};

#[must_use]
pub fn battery_profile(platform: Platform) -> Option<&'static BatteryProfile> {
    match platform {
        Platform::NintendoDs => Some(&NDS_BATTERY),
        Platform::GameBoyAdvance => Some(&GBA_BATTERY),
        Platform::NintendoEntertainmentSystem => Some(&NES_BATTERY),
        Platform::SuperNintendo => Some(&SNES_BATTERY),
        _ => None,
    }
}

enum Classified {
    Primary,
    Excluded,
    SaveState,
    Companion,
    Unrecognized,
    OtherContent,
}

impl BatteryProfile {
    /// Resolves one content's battery save without modifying the save root.
    ///
    /// `content_stem` is the `RetroArch` content basename without extension;
    /// for multi-disc content it is the playlist stem from
    /// [`crate::retroarch_content_stem`]. `remote_file_name` is the current
    /// `RomM` head filename, when one exists.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe stem or an unreadable save root.
    pub fn resolve(
        &'static self,
        save_root: &Path,
        content_stem: &str,
        remote_file_name: Option<&str>,
    ) -> Result<AdapterResolution, AdapterError> {
        if !safe_file_name(content_stem) {
            return Err(AdapterError::InvalidArgument(
                "content stem must be a single safe filename component".to_owned(),
            ));
        }
        let primary = format!("{content_stem}.{}", self.local_extension);
        let mut companions = Vec::new();
        let mut blocking = Vec::new();
        let mut diagnostics = Vec::new();
        let mut compatibility = CompatibilityStatus::Compatible;

        for name in same_stem_entries(save_root, content_stem)? {
            match self.classify(&name, content_stem) {
                Classified::Primary => {}
                Classified::Excluded => diagnostics.push("excluded_test_artifact".to_owned()),
                Classified::SaveState => diagnostics.push("save_state_excluded".to_owned()),
                Classified::OtherContent => diagnostics.push("other_content_ignored".to_owned()),
                Classified::Companion => {
                    blocking.push(ReadinessReason::CompanionFileUnsupported);
                    compatibility = CompatibilityStatus::Incompatible;
                    companions.push(name);
                }
                Classified::Unrecognized => {
                    blocking.push(ReadinessReason::UnrecognizedCompanion);
                    if compatibility == CompatibilityStatus::Compatible {
                        compatibility = CompatibilityStatus::Unknown;
                    }
                    companions.push(name);
                }
            }
        }
        diagnostics.sort();
        diagnostics.dedup();

        let remote_member = remote_file_name
            .filter(|name| {
                safe_file_name(name)
                    && Path::new(name)
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| {
                            self.remote_extensions
                                .contains(&value.to_ascii_lowercase().as_str())
                        })
            })
            .map_or_else(
                || format!("{content_stem}.{}", self.upload_extension),
                str::to_owned,
            );
        let mut files = vec![primary.clone()];
        companions.sort();
        files.extend(companions);
        let save_model = if files.len() == 1 {
            SaveModel::SingleFile
        } else {
            blocking.push(ReadinessReason::MultiFileTransportUnsupported);
            SaveModel::MultiFile
        };

        Ok(AdapterResolution {
            definition: self.definition,
            save_model,
            root: save_root.to_path_buf(),
            mappings: vec![FileMapping {
                local_relative_path: primary,
                remote_member: remote_member.clone(),
            }],
            files,
            upload_file_name: remote_member,
            romm_emulator: self.romm_emulator,
            blocking,
            compatibility,
            local_malformed: false,
            diagnostics,
            format: MemberFormat::Battery(self.sizes),
            remote_extensions: self.remote_extensions,
        })
    }

    fn classify(&self, name: &str, stem: &str) -> Classified {
        let suffix = &name[stem.len() + 1..];
        if suffix == self.local_extension {
            Classified::Primary
        } else if suffix.starts_with("state") {
            Classified::SaveState
        } else if suffix.contains('.') {
            // RetroArch names battery companions `<content>.<ext>`; a longer
            // suffix belongs to content whose basename extends this stem.
            Classified::OtherContent
        } else if self.excluded_extensions.contains(&suffix) {
            Classified::Excluded
        } else if self.companion_extensions.contains(&suffix) {
            Classified::Companion
        } else {
            Classified::Unrecognized
        }
    }
}

fn same_stem_entries(root: &Path, stem: &str) -> Result<Vec<String>, AdapterError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(AdapterError::io("read save root", root, error)),
    };
    let prefix = format!("{stem}.");
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| AdapterError::io("read save root", root, error))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with(&prefix) && name.len() > prefix.len() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}
