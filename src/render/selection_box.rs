//! The boxes moho clicks and drag-selects units by (MohoEngine.dll): the
//! bounds of the mesh it draws, not the sim's collision box.
//!
//! - Click (the pick round 0x104343d1): the mesh's axis-aligned box, trimmed
//!   to its top `SelectionMeshUseTopAmount` of height when that is set,
//!   else with the top `SelectionYOffset` (default 0.5) of it dropped, and
//!   scaled about its centre across by `SelectionMeshScaleX/Z`; of the units
//!   whose box the cursor's volume meets, the one whose position projects
//!   nearest the cursor. (Moho's oriented-box branch needs
//!   `SelectionUseOOBTestZoom`, 0 in every FAF blueprint.)
//! - Drag (0x104289d4): the mesh's oriented box, its extents x
//!   `SelectionMeshScaleX/Y/Z`, against the dragged volume.
//!
//! The mesh box is its rest-pose bounds (`Models::take_new_bounds`) under the
//! unit's scale and interpolated transform; until the mesh has loaded, the
//! collision box stands in.

use glam::{Quat, Vec3, Vec4};

/// An oriented box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    pub centre: Vec3,
    pub rotation: Quat,
    pub half: Vec3,
}

/// A unit's mesh box: `bounds` (mesh space: min xyz, max xyz) scaled by
/// `scale` and placed by its transform, else its collision box (`center`,
/// `half` in its frame).
pub fn unit_box(position: [f32; 3], rotation: [f32; 4], scale: [f32; 3], center: [f32; 3], half: [f32; 3], bounds: Option<[f32; 6]>) -> Obb {
    let rotation = Quat::from_array(rotation).normalize();
    let (local, half) = match bounds {
        Some(b) => {
            let (min, max, s) = (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]), Vec3::from_array(scale));
            ((min + max) * 0.5 * s, (max - min) * 0.5 * s.abs())
        }
        None => (Vec3::from_array(center), Vec3::from_array(half)),
    };
    Obb { centre: Vec3::from_array(position) + rotation * local, rotation, half }
}

/// The click box: `b`'s axis-aligned bounds, trimmed and scaled by the
/// blueprint's `pick` fields (`SelectionMeshUseTopAmount`,
/// `SelectionYOffset`, `SelectionMeshScaleX/Y/Z`), as min and max.
pub fn pick_box(b: &Obb, pick: [f32; 5]) -> (Vec3, Vec3) {
    let m = glam::Mat3::from_quat(b.rotation);
    let reach = Vec3::new(
        m.row(0).abs().dot(b.half),
        m.row(1).abs().dot(b.half),
        m.row(2).abs().dot(b.half),
    );
    let (mut min, mut max) = (b.centre - reach, b.centre + reach);
    let [top, y_offset, sx, _, sz] = pick;
    let height = max.y - min.y;
    if top > 0.0 {
        min.y += (1.0 - top) * height;
    } else {
        max.y -= height * y_offset;
    }
    for (k, s) in [(0, sx), (2, sz)] {
        if s != 1.0 {
            let c = (min[k] + max[k]) * 0.5;
            let h = (max[k] - c) * s;
            (min[k], max[k]) = (c - h, c + h);
        }
    }
    (min, max)
}

/// The drag box: `b` with its extents x `SelectionMeshScaleX/Y/Z`.
pub fn drag_box(b: &Obb, pick: [f32; 5]) -> Obb {
    Obb { half: b.half * Vec3::new(pick[2], pick[3], pick[4]), ..*b }
}

/// Where the ray from `origin` along `dir` enters the box `min`..`max`, if it
/// does (distance along `dir`).
pub fn ray_enters(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for k in 0..3 {
        if dir[k].abs() < 1e-8 {
            if origin[k] < min[k] || origin[k] > max[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((min[k] - origin[k]) / dir[k], (max[k] - origin[k]) / dir[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// The volume a screen rectangle (normalised device x0, y0, x1, y1) sees
/// through `inverse_view_proj`: its four sides and the near and far planes,
/// each (normal, offset) with the inside where `normal . p + offset >= 0`.
pub fn rect_volume(inverse_view_proj: glam::Mat4, [x0, y0, x1, y1]: [f32; 4]) -> [Vec4; 6] {
    let at = |x: f32, y: f32, z: f32| inverse_view_proj.project_point3(Vec3::new(x, y, z));
    let n = [at(x0, y0, 0.0), at(x1, y0, 0.0), at(x1, y1, 0.0), at(x0, y1, 0.0)];
    let f = [at(x0, y0, 1.0), at(x1, y0, 1.0), at(x1, y1, 1.0), at(x0, y1, 1.0)];
    let inside = (n[0] + n[2] + f[0] + f[2]) * 0.25;
    let plane = |a: Vec3, b: Vec3, c: Vec3| {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let p = normal.extend(-normal.dot(a));
        if p.dot(inside.extend(1.0)) < 0.0 { -p } else { p }
    };
    [plane(n[0], n[1], f[0]), plane(n[1], n[2], f[1]), plane(n[2], n[3], f[2]), plane(n[3], n[0], f[3]), plane(n[0], n[1], n[2]), plane(f[0], f[1], f[2])]
}

/// Whether the box meets the volume (no plane has it wholly outside).
pub fn meets(b: &Obb, volume: &[Vec4; 6]) -> bool {
    let axes = [b.rotation * Vec3::X, b.rotation * Vec3::Y, b.rotation * Vec3::Z];
    volume.iter().all(|p| {
        let n = p.truncate();
        let reach = axes[0].dot(n).abs() * b.half.x + axes[1].dot(n).abs() * b.half.y + axes[2].dot(n).abs() * b.half.z;
        n.dot(b.centre) + p.w >= -reach
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A naval factory's (`SelectionMeshUseTopAmount` 0.15) click box is
    /// its top 15%; by default the bottom half; scaled across by X / Z.
    #[test]
    fn click_boxes_trim_and_scale() {
        let b = unit_box([10.0, 0.0, 20.0], [0.0, 0.0, 0.0, 1.0], [1.0; 3], [0.0; 3], [0.0; 3], Some([-2.0, -1.0, -3.0, 2.0, 9.0, 3.0]));
        let (min, max) = pick_box(&b, [0.15, 0.5, 1.0, 1.0, 1.0]);
        assert!((min.y - 7.5).abs() < 1e-5 && (max.y - 9.0).abs() < 1e-5, "{min} {max}");
        let (min, max) = pick_box(&b, [0.0, 0.5, 1.0, 1.0, 1.0]);
        assert!((min.y + 1.0).abs() < 1e-5 && (max.y - 4.0).abs() < 1e-5, "{min} {max}");
        let (min, max) = pick_box(&b, [0.0, 0.0, 0.5, 1.0, 2.0]);
        assert!((min.x - 9.0).abs() < 1e-5 && (max.x - 11.0).abs() < 1e-5 && (min.z - 14.0).abs() < 1e-5 && (max.z - 26.0).abs() < 1e-5, "{min} {max}");
    }

    /// A mesh box placed by the unit's scale and transform; a quarter turn
    /// swaps its axis-aligned extents.
    #[test]
    fn mesh_boxes_follow_the_unit() {
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2).to_array();
        let b = unit_box([0.0; 3], turned, [2.0; 3], [0.0; 3], [0.0; 3], Some([-1.0, 0.0, -3.0, 1.0, 2.0, 3.0]));
        assert!((b.half - Vec3::new(2.0, 2.0, 6.0)).length() < 1e-5);
        let (min, max) = pick_box(&b, [0.0, 0.0, 1.0, 1.0, 1.0]);
        assert!((max.x - 6.0).abs() < 1e-4 && (max.z - 2.0).abs() < 1e-4, "{min} {max}");
        // Not loaded: the collision box.
        let c = unit_box([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3], [0.0, 0.5, 0.0], [1.0, 0.5, 1.0], None);
        assert_eq!((c.centre, c.half), (Vec3::new(0.0, 0.5, 0.0), Vec3::new(1.0, 0.5, 1.0)));
    }

    /// A box meets a volume when any of it is inside, not only its centre.
    #[test]
    fn drag_volumes_take_boxes_partly_inside() {
        let proj = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0) * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 50.0, 0.0), Vec3::ZERO, Vec3::Z);
        let volume = rect_volume(proj.inverse(), [-0.1, -0.1, 0.1, 0.1]);
        let at = |x: f32, half: f32| Obb { centre: Vec3::new(x, 0.0, 0.0), rotation: Quat::IDENTITY, half: Vec3::splat(half) };
        assert!(meets(&at(0.0, 0.5), &volume), "inside");
        assert!(!meets(&at(8.0, 0.5), &volume), "outside");
        // (The volume is about +-2.7 across down there: x 2..14 reaches in.)
        assert!(meets(&at(8.0, 6.0), &volume), "centre outside, edge inside");
        assert!(ray_enters(Vec3::new(0.0, 50.0, 0.0), Vec3::NEG_Y, Vec3::splat(-1.0), Vec3::splat(1.0)).is_some());
        assert!(ray_enters(Vec3::new(3.0, 50.0, 0.0), Vec3::NEG_Y, Vec3::splat(-1.0), Vec3::splat(1.0)).is_none());
    }
}
