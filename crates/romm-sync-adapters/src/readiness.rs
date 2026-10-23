use romm_sync_planner::{AdapterStatus, Platform, SaveModel};
use serde::{Serialize, Serializer};

pub const READINESS_SCHEMA_VERSION: u32 = 1;

/// Emulator family used for the "three systems, two families" exit gate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmulatorFamily {
    RetroArch,
    DolphinStandalone,
    RyujinxStandalone,
}

/// Validation stage from the M0 adapter matrix.
///
/// Only [`ReadinessStage::TargetValidated`] maps to a planner-validated
/// adapter. Every earlier stage stays non-executable in production discovery.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStage {
    Planned,
    DiscoveryReady,
    FixtureValidated,
    TargetValidated,
    Unsupported,
}

impl ReadinessStage {
    #[must_use]
    pub const fn adapter_status(self) -> AdapterStatus {
        match self {
            Self::TargetValidated => AdapterStatus::Validated,
            Self::Unsupported => AdapterStatus::Unsupported,
            Self::Planned | Self::DiscoveryReady | Self::FixtureValidated => {
                AdapterStatus::PendingValidation
            }
        }
    }
}

/// Stable machine-readable explanation for a blocked configuration or save.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReadinessReason {
    AwaitingTargetQualification,
    AwaitingLegalFixture,
    EmulatorUnverified,
    SaveRootUnverified,
    SaveModelUnverified,
    IdentityRulesUnverified,
    SharedMemoryCard,
    MultiFileTransportUnsupported,
    CompanionFileUnsupported,
    UnrecognizedCompanion,
    BatterySizeUnrecognized,
    GciMalformed,
    GciOwnerMismatch,
    GciRegionUnrecognized,
    MultiGciGroup,
    DolphinSlotConfigurationUnsupported,
    RemoteFormatUnsupported,
    RemoteSizeInvalid,
    PayloadMemberUnexpected,
    RomTitleMismatch,
    RomRegionMismatch,
    RomRevisionMismatch,
    MultiDiscWithoutPlaylist,
    AmbiguousContent,
    HashIdentityUnverified,
}

impl ReadinessReason {
    /// Stable wire/log representation shared by CLI, history, and future UI.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingTargetQualification => "awaiting_target_qualification",
            Self::AwaitingLegalFixture => "awaiting_legal_fixture",
            Self::EmulatorUnverified => "emulator_unverified",
            Self::SaveRootUnverified => "save_root_unverified",
            Self::SaveModelUnverified => "save_model_unverified",
            Self::IdentityRulesUnverified => "identity_rules_unverified",
            Self::SharedMemoryCard => "shared_memory_card",
            Self::MultiFileTransportUnsupported => "multi_file_transport_unsupported",
            Self::CompanionFileUnsupported => "companion_file_unsupported",
            Self::UnrecognizedCompanion => "unrecognized_companion",
            Self::BatterySizeUnrecognized => "battery_size_unrecognized",
            Self::GciMalformed => "gci_malformed",
            Self::GciOwnerMismatch => "gci_owner_mismatch",
            Self::GciRegionUnrecognized => "gci_region_unrecognized",
            Self::MultiGciGroup => "multi_gci_group",
            Self::DolphinSlotConfigurationUnsupported => "dolphin_slot_configuration_unsupported",
            Self::RemoteFormatUnsupported => "remote_format_unsupported",
            Self::RemoteSizeInvalid => "remote_size_invalid",
            Self::PayloadMemberUnexpected => "payload_member_unexpected",
            Self::RomTitleMismatch => "rom_title_mismatch",
            Self::RomRegionMismatch => "rom_region_mismatch",
            Self::RomRevisionMismatch => "rom_revision_mismatch",
            Self::MultiDiscWithoutPlaylist => "multi_disc_without_playlist",
            Self::AmbiguousContent => "ambiguous_content",
            Self::HashIdentityUnverified => "hash_identity_unverified",
        }
    }
}

impl Serialize for ReadinessReason {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One concrete platform/emulator/core/profile configuration.
///
/// Configurations advance independently. A definition's stage is a reviewed
/// source-code fact, never a runtime setting, so configuration files cannot
/// accidentally enable automatic writes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterDefinition {
    pub configuration_id: &'static str,
    pub revision: &'static str,
    pub platform: Platform,
    pub family: EmulatorFamily,
    pub emulator: &'static str,
    pub save_model: SaveModel,
    pub stage: ReadinessStage,
    pub reasons: &'static [ReadinessReason],
    /// Transfer directions the adapter can represent once validated.
    pub can_upload: bool,
    pub can_download: bool,
}

impl AdapterDefinition {
    #[must_use]
    pub const fn status(&self) -> AdapterStatus {
        self.stage.adapter_status()
    }

    /// Whether disposable qualification may treat this adapter as validated.
    #[must_use]
    pub fn qualifiable(&self) -> bool {
        matches!(
            self.stage,
            ReadinessStage::FixtureValidated | ReadinessStage::TargetValidated
        ) && (self.can_upload || self.can_download)
    }
}

use ReadinessReason as R;

pub const NDS_MELONDS_DS: AdapterDefinition = AdapterDefinition {
    configuration_id: "retroarch-melondsds-nds",
    revision: "m3-1",
    platform: Platform::NintendoDs,
    family: EmulatorFamily::RetroArch,
    emulator: "RetroArch melonDS DS 1.4.0",
    save_model: SaveModel::SingleFile,
    stage: ReadinessStage::FixtureValidated,
    reasons: &[R::AwaitingTargetQualification],
    can_upload: true,
    can_download: true,
};

pub const GBA_MGBA: AdapterDefinition = AdapterDefinition {
    configuration_id: "retroarch-mgba-gba",
    revision: "m3-1",
    platform: Platform::GameBoyAdvance,
    family: EmulatorFamily::RetroArch,
    emulator: "RetroArch mGBA 0.10-dev",
    save_model: SaveModel::SingleFile,
    stage: ReadinessStage::FixtureValidated,
    reasons: &[R::AwaitingTargetQualification, R::AwaitingLegalFixture],
    can_upload: true,
    can_download: true,
};

pub const NGC_DOLPHIN_GCI: AdapterDefinition = AdapterDefinition {
    configuration_id: "dolphin-gci-folder-ngc",
    revision: "m3-1",
    platform: Platform::GameCube,
    family: EmulatorFamily::DolphinStandalone,
    emulator: "Dolphin (Standalone) 2609a, Slot A GCI Folder, Slot B None",
    save_model: SaveModel::SingleFile,
    stage: ReadinessStage::FixtureValidated,
    reasons: &[
        R::AwaitingTargetQualification,
        R::MultiFileTransportUnsupported,
    ],
    can_upload: true,
    can_download: true,
};

const fn planned(
    configuration_id: &'static str,
    platform: Platform,
    family: EmulatorFamily,
    emulator: &'static str,
    save_model: SaveModel,
    reasons: &'static [ReadinessReason],
) -> AdapterDefinition {
    AdapterDefinition {
        configuration_id,
        revision: "m3-1",
        platform,
        family,
        emulator,
        save_model,
        stage: ReadinessStage::Planned,
        reasons,
        can_upload: false,
        can_download: false,
    }
}

const fn unsupported(
    configuration_id: &'static str,
    platform: Platform,
    emulator: &'static str,
    reasons: &'static [ReadinessReason],
) -> AdapterDefinition {
    AdapterDefinition {
        configuration_id,
        revision: "m3-1",
        platform,
        family: EmulatorFamily::RetroArch,
        emulator,
        save_model: SaveModel::SharedMemoryCard,
        stage: ReadinessStage::Unsupported,
        reasons,
        can_upload: false,
        can_download: false,
    }
}

pub const NES_MESEN: AdapterDefinition = planned(
    "retroarch-mesen-nes",
    Platform::NintendoEntertainmentSystem,
    EmulatorFamily::RetroArch,
    "RetroArch Mesen",
    SaveModel::SingleFile,
    &[R::EmulatorUnverified, R::SaveRootUnverified],
);

pub const SNES_SNES9X: AdapterDefinition = planned(
    "retroarch-snes9x-snes",
    Platform::SuperNintendo,
    EmulatorFamily::RetroArch,
    "RetroArch Snes9x - Current",
    SaveModel::SingleFile,
    &[
        R::EmulatorUnverified,
        R::SaveRootUnverified,
        R::SaveModelUnverified,
    ],
);

/// Every concrete configuration represented by M3, ordered by the approved
/// rollout priority. Each of the twelve platforms has at least one entry.
pub const ADAPTERS: &[AdapterDefinition] = &[
    NDS_MELONDS_DS,
    GBA_MGBA,
    NGC_DOLPHIN_GCI,
    NES_MESEN,
    SNES_SNES9X,
    planned(
        "retroarch-beetle-psx-per-game-card-psx",
        Platform::PlayStation,
        EmulatorFamily::RetroArch,
        "RetroArch Beetle PSX",
        SaveModel::GameSpecificMemoryCard,
        &[
            R::EmulatorUnverified,
            R::SaveModelUnverified,
            R::IdentityRulesUnverified,
        ],
    ),
    unsupported(
        "retroarch-beetle-psx-shared-card-psx",
        Platform::PlayStation,
        "RetroArch Beetle PSX shared memory card",
        &[R::SharedMemoryCard],
    ),
    planned(
        "retroarch-mupen64plus-next-n64",
        Platform::Nintendo64,
        EmulatorFamily::RetroArch,
        "RetroArch Mupen64Plus-Next",
        SaveModel::MultiFile,
        &[
            R::EmulatorUnverified,
            R::SaveModelUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
    planned(
        "retroarch-ppsspp-psp",
        Platform::PlayStationPortable,
        EmulatorFamily::RetroArch,
        "RetroArch PPSSPP",
        SaveModel::DirectoryTree,
        &[
            R::EmulatorUnverified,
            R::SaveRootUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
    planned(
        "dolphin-wii-title-tree-wii",
        Platform::Wii,
        EmulatorFamily::DolphinStandalone,
        "Dolphin (Standalone) Wii NAND",
        SaveModel::DirectoryTree,
        &[
            R::SaveModelUnverified,
            R::IdentityRulesUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
    planned(
        "retroarch-citra-3ds",
        Platform::Nintendo3Ds,
        EmulatorFamily::RetroArch,
        "RetroArch Citra (core unresolved)",
        SaveModel::DirectoryTree,
        &[
            R::EmulatorUnverified,
            R::SaveRootUnverified,
            R::IdentityRulesUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
    planned(
        "retroarch-pcee2-folder-card-ps2",
        Platform::PlayStation2,
        EmulatorFamily::RetroArch,
        "RetroArch PCEE2",
        SaveModel::DirectoryTree,
        &[
            R::EmulatorUnverified,
            R::SaveModelUnverified,
            R::IdentityRulesUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
    unsupported(
        "retroarch-pcee2-shared-card-ps2",
        Platform::PlayStation2,
        "RetroArch PCEE2 shared memory card",
        &[R::SharedMemoryCard],
    ),
    planned(
        "ryujinx-user-save-switch",
        Platform::NintendoSwitch,
        EmulatorFamily::RyujinxStandalone,
        "Ryujinx (Standalone)",
        SaveModel::DirectoryTree,
        &[
            R::EmulatorUnverified,
            R::SaveModelUnverified,
            R::IdentityRulesUnverified,
            R::MultiFileTransportUnsupported,
        ],
    ),
];

#[must_use]
pub fn adapter_definition(configuration_id: &str) -> Option<&'static AdapterDefinition> {
    ADAPTERS
        .iter()
        .find(|definition| definition.configuration_id == configuration_id)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReadinessReport {
    pub schema_version: u32,
    pub automatic_writes_enabled: Vec<String>,
    pub platforms: Vec<PlatformReadiness>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlatformReadiness {
    pub platform: String,
    pub configurations: Vec<ConfigurationReadiness>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigurationReadiness {
    pub configuration_id: String,
    pub revision: String,
    pub family: EmulatorFamily,
    pub emulator: String,
    pub save_model: String,
    pub stage: ReadinessStage,
    pub automatic_writes: bool,
    pub reasons: Vec<ReadinessReason>,
}

/// Builds the inventory of all twelve platforms with their blocking reasons.
#[must_use]
pub fn readiness_report() -> ReadinessReport {
    let platforms = Platform::ALL
        .iter()
        .map(|platform| PlatformReadiness {
            platform: platform.slug().to_owned(),
            configurations: ADAPTERS
                .iter()
                .filter(|definition| definition.platform == *platform)
                .map(|definition| ConfigurationReadiness {
                    configuration_id: definition.configuration_id.to_owned(),
                    revision: definition.revision.to_owned(),
                    family: definition.family,
                    emulator: definition.emulator.to_owned(),
                    save_model: save_model_name(definition.save_model).to_owned(),
                    stage: definition.stage,
                    automatic_writes: definition.status() == AdapterStatus::Validated,
                    reasons: definition.reasons.to_vec(),
                })
                .collect(),
        })
        .collect();
    ReadinessReport {
        schema_version: READINESS_SCHEMA_VERSION,
        automatic_writes_enabled: ADAPTERS
            .iter()
            .filter(|definition| definition.status() == AdapterStatus::Validated)
            .map(|definition| definition.configuration_id.to_owned())
            .collect(),
        platforms,
    }
}

#[must_use]
pub const fn save_model_name(model: SaveModel) -> &'static str {
    match model {
        SaveModel::SingleFile => "single_file",
        SaveModel::MultiFile => "multi_file",
        SaveModel::GameSpecificMemoryCard => "game_specific_memory_card",
        SaveModel::DirectoryTree => "directory_tree",
        SaveModel::SharedMemoryCard => "shared_memory_card",
        SaveModel::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_platform_is_represented_and_ids_are_unique() {
        let ids: BTreeSet<_> = ADAPTERS.iter().map(|d| d.configuration_id).collect();
        assert_eq!(ids.len(), ADAPTERS.len());
        for platform in Platform::ALL {
            assert!(
                ADAPTERS.iter().any(|d| d.platform == platform),
                "{} has no configuration",
                platform.slug()
            );
        }
        let report = readiness_report();
        assert_eq!(report.platforms.len(), 12);
        assert!(
            report
                .platforms
                .iter()
                .all(|p| !p.configurations.is_empty())
        );
    }

    #[test]
    fn nothing_is_enabled_for_automatic_writes_without_target_validation() {
        for definition in ADAPTERS {
            assert_eq!(
                definition.status() == AdapterStatus::Validated,
                definition.stage == ReadinessStage::TargetValidated
            );
            assert!(
                !definition.reasons.is_empty()
                    || definition.stage == ReadinessStage::TargetValidated
            );
        }
        assert_eq!(
            readiness_report().automatic_writes_enabled,
            Vec::<String>::new()
        );
    }

    #[test]
    fn shared_cards_are_unsupported_and_unvalidated_rows_cannot_transfer() {
        for definition in ADAPTERS {
            if definition.save_model == SaveModel::SharedMemoryCard {
                assert_eq!(definition.stage, ReadinessStage::Unsupported);
                assert!(
                    definition
                        .reasons
                        .contains(&ReadinessReason::SharedMemoryCard)
                );
            }
            if definition.stage < ReadinessStage::FixtureValidated
                || definition.stage == ReadinessStage::Unsupported
            {
                assert!(!definition.can_upload && !definition.can_download);
                assert!(!definition.qualifiable());
            }
        }
    }

    #[test]
    fn early_validation_order_spans_two_families() {
        let qualifiable: Vec<_> = ADAPTERS.iter().filter(|d| d.qualifiable()).collect();
        let ids: Vec<_> = qualifiable.iter().map(|d| d.configuration_id).collect();
        assert_eq!(
            ids,
            [
                "retroarch-melondsds-nds",
                "retroarch-mgba-gba",
                "dolphin-gci-folder-ngc"
            ]
        );
        let families: BTreeSet<_> = qualifiable.iter().map(|d| d.family).collect();
        assert!(families.len() >= 2);
    }

    #[test]
    fn reasons_serialize_as_stable_codes() {
        let encoded = serde_json::to_string(&ReadinessReason::SharedMemoryCard).unwrap();
        assert_eq!(encoded, "\"shared_memory_card\"");
    }
}
