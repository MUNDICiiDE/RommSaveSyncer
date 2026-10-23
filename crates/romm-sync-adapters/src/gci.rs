use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use romm_sync_core::FileMapping;
use romm_sync_planner::{CompatibilityStatus, SaveModel};

use crate::{
    AdapterError, AdapterResolution, NGC_DOLPHIN_GCI, ReadinessReason,
    resolution::{MemberFormat, safe_file_name},
};

/// Size of the GCI directory entry that precedes the save blocks.
pub const GCI_HEADER_LEN: u64 = 0x40;
/// Size of one `GameCube` memory-card block.
pub const GCI_BLOCK_LEN: u64 = 0x2000;
/// Largest user-data block count on a 2043-block memory card.
pub const GCI_MAX_BLOCKS: u64 = 2043;

/// Six-byte disc identity: four-byte game code plus two-byte maker code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GameId([u8; 6]);

impl GameId {
    /// Parses an identity such as `GKYE01` from Dolphin's disc header.
    ///
    /// # Errors
    ///
    /// Returns an error unless the value is six uppercase ASCII alphanumerics.
    pub fn parse(value: &str) -> Result<Self, AdapterError> {
        let bytes: [u8; 6] = value
            .as_bytes()
            .try_into()
            .map_err(|_| AdapterError::InvalidArgument("game ID must be six bytes".to_owned()))?;
        if !bytes.iter().copied().all(valid_code_byte) {
            return Err(AdapterError::InvalidArgument(
                "game ID must be uppercase ASCII letters and digits".to_owned(),
            ));
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn game_code(&self) -> &[u8] {
        &self.0[..4]
    }

    #[must_use]
    pub fn maker_code(&self) -> &[u8] {
        &self.0[4..]
    }

    #[must_use]
    pub fn region(&self) -> Option<GciRegion> {
        GciRegion::from_country(self.0[3])
    }
}

impl fmt::Display for GameId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(std::str::from_utf8(&self.0).unwrap_or("??????"))
    }
}

/// Dolphin's `GameCube` user-directory regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GciRegion {
    Usa,
    Europe,
    Japan,
}

impl GciRegion {
    /// Maps the game-code country byte the way Dolphin selects a GC folder.
    /// Unlisted countries, including NTSC-K, are not guessed.
    #[must_use]
    pub const fn from_country(country: u8) -> Option<Self> {
        match country {
            b'E' => Some(Self::Usa),
            b'J' => Some(Self::Japan),
            b'P' | b'D' | b'F' | b'I' | b'S' | b'H' | b'U' | b'X' | b'Y' => Some(Self::Europe),
            _ => None,
        }
    }

    #[must_use]
    pub const fn directory(self) -> &'static str {
        match self {
            Self::Usa => "USA",
            Self::Europe => "EUR",
            Self::Japan => "JAP",
        }
    }
}

/// Parsed, structurally valid GCI directory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GciHeader {
    pub game_code: [u8; 4],
    pub maker_code: [u8; 2],
    pub internal_name: String,
    pub block_count: u16,
}

impl GciHeader {
    #[must_use]
    pub fn owned_by(&self, game_id: &GameId) -> bool {
        self.game_code == game_id.game_code() && self.maker_code == game_id.maker_code()
    }
}

/// Parses and structurally validates a complete `.gci` file.
///
/// # Errors
///
/// Returns [`ReadinessReason::GciMalformed`] for a truncated header, invalid
/// game/maker code, impossible block count, or a length that the declared
/// block count does not exactly account for.
pub fn parse_gci(bytes: &[u8]) -> Result<GciHeader, ReadinessReason> {
    let header = bytes.get(..0x40).ok_or(ReadinessReason::GciMalformed)?;
    let game_code: [u8; 4] = header[..4]
        .try_into()
        .map_err(|_| ReadinessReason::GciMalformed)?;
    let maker_code: [u8; 2] = header[4..6]
        .try_into()
        .map_err(|_| ReadinessReason::GciMalformed)?;
    if !game_code
        .iter()
        .chain(&maker_code)
        .copied()
        .all(valid_code_byte)
    {
        return Err(ReadinessReason::GciMalformed);
    }
    let block_count = u16::from_be_bytes([header[0x38], header[0x39]]);
    if block_count == 0 || u64::from(block_count) > GCI_MAX_BLOCKS {
        return Err(ReadinessReason::GciMalformed);
    }
    if bytes.len() as u64 != GCI_HEADER_LEN + u64::from(block_count) * GCI_BLOCK_LEN {
        return Err(ReadinessReason::GciMalformed);
    }
    let name = &header[0x08..0x28];
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    Ok(GciHeader {
        game_code,
        maker_code,
        internal_name: String::from_utf8_lossy(&name[..end]).into_owned(),
        block_count,
    })
}

/// Validates that `bytes` is a complete GCI owned by `game_id`.
///
/// # Errors
///
/// Returns a malformed or owner-mismatch reason.
pub fn validate_gci_owner(bytes: &[u8], game_id: &GameId) -> Result<(), ReadinessReason> {
    let header = parse_gci(bytes)?;
    if header.owned_by(game_id) {
        Ok(())
    } else {
        Err(ReadinessReason::GciOwnerMismatch)
    }
}

#[must_use]
pub fn plausible_gci_length(size: u64) -> bool {
    size > GCI_HEADER_LEN
        && (size - GCI_HEADER_LEN).is_multiple_of(GCI_BLOCK_LEN)
        && (size - GCI_HEADER_LEN) / GCI_BLOCK_LEN <= GCI_MAX_BLOCKS
}

/// Dolphin memory-card slot device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardDevice {
    GciFolder,
    RawCard,
    None,
    Other,
}

/// Dolphin's configured `GameCube` memory-card devices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DolphinCardConfig {
    pub slot_a: CardDevice,
    pub slot_b: CardDevice,
}

impl DolphinCardConfig {
    /// The configuration observed during M0: Slot A GCI Folder, Slot B None.
    pub const AUDITED: Self = Self {
        slot_a: CardDevice::GciFolder,
        slot_b: CardDevice::None,
    };
}

const ADAPTER_PLACEHOLDER: &str = "romm-autosync";

/// Resolves one game's GCI files in Dolphin's per-region Slot A GCI folder.
///
/// `gc_root` is Dolphin's `GC` user directory. Ownership comes from each GCI
/// header, never from its filename. `remote_file_name` names the download
/// target when the game has no local GCI yet.
///
/// # Errors
///
/// Returns an error for an invalid game ID or an unreadable folder.
pub fn resolve_gci(
    gc_root: &Path,
    game_id: &str,
    cards: DolphinCardConfig,
    remote_file_name: Option<&str>,
) -> Result<AdapterResolution, AdapterError> {
    let game_id = GameId::parse(game_id)?;
    let mut blocking = Vec::new();
    let mut diagnostics = Vec::new();
    let mut compatibility = CompatibilityStatus::Compatible;
    let mut local_malformed = false;

    if cards != DolphinCardConfig::AUDITED {
        blocking.push(ReadinessReason::DolphinSlotConfigurationUnsupported);
        compatibility = CompatibilityStatus::Incompatible;
    }
    let root = if let Some(region) = game_id.region() {
        gc_root.join(region.directory()).join("Card A")
    } else {
        blocking.push(ReadinessReason::GciRegionUnrecognized);
        compatibility = CompatibilityStatus::Incompatible;
        gc_root.to_path_buf()
    };

    let mut owned = Vec::new();
    if game_id.region().is_some() {
        for entry in gci_entries(&root)? {
            match entry {
                FolderEntry::Gci { name, header } => {
                    if header.owned_by(&game_id) {
                        owned.push(name);
                    } else if header.game_code[..3] == game_id.game_code()[..3]
                        && header.maker_code == game_id.maker_code()
                    {
                        diagnostics.push("same_title_other_region_gci_ignored".to_owned());
                    }
                }
                FolderEntry::Malformed => {
                    // An unparseable GCI could belong to this game; ownership of
                    // the folder can no longer be proven complete.
                    blocking.push(ReadinessReason::GciMalformed);
                    local_malformed = true;
                }
                FolderEntry::Other => diagnostics.push("non_gci_entry_ignored".to_owned()),
            }
        }
    }
    owned.sort();
    diagnostics.sort();
    diagnostics.dedup();

    let save_model = if owned.len() > 1 {
        blocking.push(ReadinessReason::MultiGciGroup);
        blocking.push(ReadinessReason::MultiFileTransportUnsupported);
        compatibility = CompatibilityStatus::Incompatible;
        SaveModel::MultiFile
    } else {
        SaveModel::SingleFile
    };
    let files = if owned.is_empty() {
        vec![
            remote_file_name
                .filter(|name| safe_file_name(name) && has_gci_extension(name))
                .map_or_else(
                    || {
                        format!(
                            "{}-{}-{ADAPTER_PLACEHOLDER}.gci",
                            String::from_utf8_lossy(game_id.maker_code()),
                            String::from_utf8_lossy(game_id.game_code())
                        )
                    },
                    str::to_owned,
                ),
        ]
    } else {
        owned
    };
    let mappings = files
        .iter()
        .map(|name| FileMapping {
            local_relative_path: name.clone(),
            remote_member: name.clone(),
        })
        .collect();

    Ok(AdapterResolution {
        definition: &NGC_DOLPHIN_GCI,
        save_model,
        root,
        upload_file_name: files[0].clone(),
        files,
        mappings,
        romm_emulator: "dolphin",
        blocking,
        compatibility,
        local_malformed,
        diagnostics,
        format: MemberFormat::Gci(game_id),
        remote_extensions: &["gci"],
    })
}

enum FolderEntry {
    Gci { name: String, header: GciHeader },
    Malformed,
    Other,
}

fn has_gci_extension(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("gci"))
}

fn gci_entries(folder: &Path) -> Result<Vec<FolderEntry>, AdapterError> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(AdapterError::io("read GCI folder", folder, error)),
    };
    let max_len = GCI_HEADER_LEN + GCI_MAX_BLOCKS * GCI_BLOCK_LEN;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| AdapterError::io("read GCI folder", folder, error))?;
        let path: PathBuf = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            found.push(FolderEntry::Other);
            continue;
        };
        if name.starts_with(".romm-autosync") {
            continue;
        }
        if !has_gci_extension(&name) {
            found.push(FolderEntry::Other);
            continue;
        }
        let metadata =
            fs::metadata(&path).map_err(|error| AdapterError::io("inspect GCI", &path, error))?;
        if !metadata.is_file() || metadata.len() > max_len {
            found.push(FolderEntry::Malformed);
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| AdapterError::io("read GCI", &path, error))?;
        found.push(match parse_gci(&bytes) {
            Ok(header) => FolderEntry::Gci { name, header },
            Err(_) => FolderEntry::Malformed,
        });
    }
    Ok(found)
}

const fn valid_code_byte(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_digit()
}
