use std::path::Path;

use romm_sync_planner::IdentityStatus;

use crate::ReadinessReason;

const REGIONS: &[&str] = &[
    "asia",
    "australia",
    "brazil",
    "canada",
    "china",
    "europe",
    "france",
    "germany",
    "italy",
    "japan",
    "korea",
    "netherlands",
    "spain",
    "sweden",
    "taiwan",
    "uk",
    "usa",
    "world",
];

/// No-Intro/Redump-style tags parsed from a ROM filename.
///
/// Tags never establish identity on their own. They exist so a hash-verified
/// match can still fail closed when the local and `RomM` names disagree about
/// region or revision.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RomNameTags {
    pub title: String,
    pub regions: Vec<String>,
    pub revision: Option<String>,
    pub disc: Option<u32>,
}

/// Parses title, region, revision, and disc tags from a ROM filename.
#[must_use]
pub fn parse_rom_name(file_name: &str) -> RomNameTags {
    let base = strip_extension(file_name);
    let title_end = base.find(['(', '[']).unwrap_or(base.len());
    let mut tags = RomNameTags {
        title: normalize_words(&base[..title_end]),
        ..RomNameTags::default()
    };
    let mut rest = &base[title_end..];
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let tag = rest[open + 1..open + close].trim();
        rest = &rest[open + close + 1..];
        let lower = tag.to_ascii_lowercase();
        if let Some(number) = lower.strip_prefix("disc ") {
            tags.disc = number.trim().parse().ok();
        } else if let Some(revision) = lower.strip_prefix("rev ") {
            tags.revision = Some(revision.trim().to_owned());
        } else if let Some(version) = lower.strip_prefix('v')
            && !version.is_empty()
            && version.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            tags.revision = Some(version.to_owned());
        } else {
            let parts: Vec<_> = lower.split(',').map(str::trim).collect();
            if !parts.is_empty() && parts.iter().all(|part| REGIONS.contains(part)) {
                tags.regions.extend(parts.into_iter().map(str::to_owned));
            }
        }
    }
    tags.regions.sort();
    tags.regions.dedup();
    tags
}

/// Compares the local ROM name with the `RomM` ROM name.
///
/// # Errors
///
/// Returns a stable reason for title, region, or revision disagreement. Disc
/// numbers are ignored because all discs of one game share its save.
pub fn compare_rom_names(local: &str, romm: &str) -> Result<(), ReadinessReason> {
    let local = parse_rom_name(local);
    let romm = parse_rom_name(romm);
    if local.title != romm.title {
        return Err(ReadinessReason::RomTitleMismatch);
    }
    if !local.regions.is_empty() && !romm.regions.is_empty() && local.regions != romm.regions {
        return Err(ReadinessReason::RomRegionMismatch);
    }
    if local.revision != romm.revision {
        return Err(ReadinessReason::RomRevisionMismatch);
    }
    Ok(())
}

/// Combines hash evidence with filename consistency into planner identity.
///
/// A hash match against exactly one `RomM` ROM is required; a consistent name
/// alone is never sufficient.
#[must_use]
pub fn assess_identity(
    unique_hash_match: bool,
    local_rom_name: &str,
    romm_rom_name: &str,
) -> (IdentityStatus, Option<ReadinessReason>) {
    if !unique_hash_match {
        return (
            IdentityStatus::Ambiguous,
            Some(ReadinessReason::HashIdentityUnverified),
        );
    }
    match compare_rom_names(local_rom_name, romm_rom_name) {
        Ok(()) => (IdentityStatus::Verified, None),
        Err(reason) => (IdentityStatus::Ambiguous, Some(reason)),
    }
}

/// Selects the `RetroArch` content basename that owns a game's battery save.
///
/// A single file uses its own stem. Multi-disc content must be launched from
/// exactly one `.m3u` playlist so every disc shares one save; individual disc
/// files would otherwise split progress across several saves.
///
/// # Errors
///
/// Returns a reason when the content files cannot be attributed to one save.
pub fn retroarch_content_stem(content_files: &[&str]) -> Result<String, ReadinessReason> {
    let names: Vec<&str> = content_files
        .iter()
        .map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
        })
        .collect();
    match names.as_slice() {
        [] => Err(ReadinessReason::AmbiguousContent),
        [single] => Ok(strip_extension(single).to_owned()),
        many => {
            let playlists: Vec<_> = many
                .iter()
                .filter(|name| has_extension(name, "m3u"))
                .collect();
            let discs: Vec<_> = many
                .iter()
                .filter(|name| !has_extension(name, "m3u"))
                .map(|name| parse_rom_name(name))
                .collect();
            if discs.iter().any(|tags| tags.disc.is_none()) {
                return Err(ReadinessReason::AmbiguousContent);
            }
            let first = &discs[0];
            if discs.iter().any(|tags| tags.title != first.title) {
                return Err(ReadinessReason::RomTitleMismatch);
            }
            if discs.iter().any(|tags| tags.regions != first.regions) {
                return Err(ReadinessReason::RomRegionMismatch);
            }
            if discs.iter().any(|tags| tags.revision != first.revision) {
                return Err(ReadinessReason::RomRevisionMismatch);
            }
            match playlists.as_slice() {
                [playlist] => Ok(strip_extension(playlist).to_owned()),
                [] => Err(ReadinessReason::MultiDiscWithoutPlaylist),
                _ => Err(ReadinessReason::AmbiguousContent),
            }
        }
    }
}

fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn strip_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(index) if index > 0 && !name[index + 1..].contains([' ', ')', ']']) => &name[..index],
        _ => name,
    }
}

fn normalize_words(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
