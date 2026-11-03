//! Navigation / pathfinding settings owned by the sim.

/// Tunables set from Lua (sim state: identical on every client).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NavigatorConfig {
    /// Move orders whose target is closer than this use personal positioning
    /// instead of formation waypoints (`SetNavigatorPersonalPosMaxDistance`).
    pub personal_pos_max_distance: f64,
}

impl Default for NavigatorConfig {
    fn default() -> Self {
        // Moho default (see /lua/simInit.lua SetupPathfinding).
        Self { personal_pos_max_distance: 50.0 }
    }
}
