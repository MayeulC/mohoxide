//! Intel (vision, radar, sonar, omni, stealth, jamming, cloaking).

use std::collections::BTreeMap;

use crate::engine::sim::math::Fx;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub enum IntelType {
    Vision,
    WaterVision,
    Radar,
    Sonar,
    Omni,
    RadarStealthField,
    SonarStealthField,
    CloakField,
    Jammer,
    Spoof,
    Cloak,
    RadarStealth,
    SonarStealth,
}

impl IntelType {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "Vision" => Self::Vision,
            "WaterVision" => Self::WaterVision,
            "Radar" => Self::Radar,
            "Sonar" => Self::Sonar,
            "Omni" => Self::Omni,
            "RadarStealthField" => Self::RadarStealthField,
            "SonarStealthField" => Self::SonarStealthField,
            "CloakField" => Self::CloakField,
            "Jammer" => Self::Jammer,
            "Spoof" => Self::Spoof,
            "Cloak" => Self::Cloak,
            "RadarStealth" => Self::RadarStealth,
            "SonarStealth" => Self::SonarStealth,
            _ => return None,
        })
    }

    /// Blueprint `Intel` field holding this type's radius, if any.
    pub fn radius_field(self) -> Option<&'static str> {
        Some(match self {
            Self::Vision => "VisionRadius",
            Self::WaterVision => "WaterVisionRadius",
            Self::Radar => "RadarRadius",
            Self::Sonar => "SonarRadius",
            Self::Omni => "OmniRadius",
            Self::RadarStealthField => "RadarStealthFieldRadius",
            Self::SonarStealthField => "SonarStealthFieldRadius",
            Self::CloakField => "CloakFieldRadius",
            _ => return None,
        })
    }

    pub const ALL: [Self; 13] = [
        Self::Vision,
        Self::WaterVision,
        Self::Radar,
        Self::Sonar,
        Self::Omni,
        Self::RadarStealthField,
        Self::SonarStealthField,
        Self::CloakField,
        Self::Jammer,
        Self::Spoof,
        Self::Cloak,
        Self::RadarStealth,
        Self::SonarStealth,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IntelState {
    pub enabled: bool,
    pub radius: Fx,
}

/// An entity's intel, type-ordered.
pub type IntelMap = BTreeMap<IntelType, IntelState>;
