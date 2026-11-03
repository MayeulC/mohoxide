//! Fixed-point sim math. All sim state positions/quantities use [`Fx`] so the
//! sim is bit-identical across platforms; Lua numbers (f64) are converted at
//! the API boundary (`from_num` rounds to nearest — exact and deterministic).

pub type Fx = fixed::types::I32F32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FxVec2 {
    pub x: Fx,
    pub z: Fx,
}

impl FxVec2 {
    pub fn from_f64(x: f64, z: f64) -> Self {
        Self { x: from_lua(x), z: from_lua(z) }
    }

    pub fn to_f64(self) -> (f64, f64) {
        (self.x.to_num(), self.z.to_num())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FxVec3 {
    pub x: Fx,
    pub y: Fx,
    pub z: Fx,
}

impl std::ops::Add for FxVec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self { x: self.x + o.x, y: self.y + o.y, z: self.z + o.z }
    }
}

impl std::ops::Mul<Fx> for FxVec3 {
    type Output = Self;
    fn mul(self, s: Fx) -> Self {
        Self { x: self.x * s, y: self.y * s, z: self.z * s }
    }
}

impl std::ops::Sub for FxVec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self { x: self.x - o.x, y: self.y - o.y, z: self.z - o.z }
    }
}

impl FxVec3 {
    pub fn length(self) -> Fx {
        let sq = |v: Fx| v.saturating_mul(v);
        sqrt(sq(self.x).saturating_add(sq(self.y)).saturating_add(sq(self.z)))
    }

    /// Unit vector in this direction, `None` for the zero vector.
    pub fn normalized(self) -> Option<Self> {
        let len = self.length();
        // One (i128) division, then multiplies.
        (len > Fx::ZERO).then(|| self * (Fx::ONE / len))
    }

    pub fn from_f64(x: f64, y: f64, z: f64) -> Self {
        Self { x: from_lua(x), y: from_lua(y), z: from_lua(z) }
    }

    pub fn to_f64(self) -> (f64, f64, f64) {
        (self.x.to_num(), self.y.to_num(), self.z.to_num())
    }
}

/// Square root, bit-identical to `Fx::sqrt` (floor of the exact root) but
/// ~5x faster. The f64 square root only seeds the integer root; integer
/// checks then correct it to exactly `floor(sqrt(bits << 32))`, so the result
/// never depends on platform floating point (docs/architecture/determinism.md). Negative
/// input -> 0.
/// A Lua number as fixed point: saturates out-of-range values and reads NaN
/// as 0 (FAF scripts occasionally produce NaN, e.g. bomber ballistics on a
/// degenerate drop; a panic would take the sim down, and 0 is the same on
/// every machine).
pub fn from_lua(v: f64) -> Fx {
    if v.is_nan() { Fx::ZERO } else { Fx::saturating_from_num(v) }
}

pub fn sqrt(v: Fx) -> Fx {
    if v <= Fx::ZERO {
        return Fx::ZERO;
    }
    let n = (v.to_bits() as u128) << 32;
    let mut r = (n as f64).sqrt() as u128;
    while r * r > n {
        r -= 1;
    }
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    Fx::from_bits(r as i64)
}

/// Deterministic `atan2(y, x)` in integer arithmetic (error under 5e-8
/// rad): reduced to one octant, `t = min / max`, then Abramowitz & Stegun
/// 4.4.49's odd polynomial for atan on [0, 1] (Horner, I32F32 bits in
/// i128). Exact 0 on the +x axis and at the origin, exact ±pi/2 on the y
/// axis and pi on the -x axis. Replaced the `cordic` crate
/// (weapon aiming 234 -> 149 us per 2000 weapons).
pub fn atan2(y: Fx, x: Fx) -> Fx {
    if y == Fx::ZERO && x >= Fx::ZERO {
        return Fx::ZERO;
    }
    // A&S 4.4.49 coefficients (a1, a3, ..., a15) as I32F32 bits.
    const C: [i128; 8] = [
        4_294_964_431,  // 0.9999993329
        -1_431_506_417, // -0.3332985605
        856_697_197,    // 0.1994653599
        -597_366_966,   // -0.1390853351
        414_120_936,    // 0.0964200441
        -240_131_132,   // -0.0559098861
        93_893_263,     // 0.0218612288
        -17_412_047,    // -0.0040540580
    ];
    let (ax, ay) = ((x.to_bits() as i128).abs(), (y.to_bits() as i128).abs());
    let swap = ay > ax;
    let (num, den) = if swap { (ax, ay) } else { (ay, ax) };
    let t = (num << 32) / den;
    let t2 = (t * t) >> 32;
    let mut p = C[7];
    for &c in C[..7].iter().rev() {
        p = ((p * t2) >> 32) + c;
    }
    let mut r = (p * t) >> 32;
    let pi = Fx::PI.to_bits() as i128;
    if swap {
        r = pi / 2 - r;
    }
    if x < Fx::ZERO {
        r = pi - r;
    }
    if y < Fx::ZERO {
        r = -r;
    }
    Fx::from_bits(r as i64)
}

/// Deterministic `(sin, cos)` in integer arithmetic (error under 2e-9):
/// reduced by quarter turns to [-pi/4, pi/4], then Taylor polynomials to
/// r^11 / r^12 (Horner, I32F32 bits in i128). Exact for zero. 65 -> 12 ns
/// against the `cordic` crate it replaced.
pub fn sin_cos(a: Fx) -> (Fx, Fx) {
    if a == Fx::ZERO {
        return (Fx::ZERO, Fx::ONE);
    }
    // (-1)^k / (2k+1)! and (-1)^k / (2k)! as I32F32 bits.
    const S: [i128; 6] = [4294967296, -715827883, 35791394, -852176, 11836, -108];
    const C: [i128; 7] = [4294967296, -2147483648, 178956971, -5965232, 106522, -1184, 9];
    // pi/2 and 2/pi as I32F32 bits.
    const HALF_PI: i128 = 6746518852;
    const TWO_OVER_PI: i128 = 2734261102;
    let x = a.to_bits() as i128;
    // Nearest quarter turn, then the remainder within it.
    let k = ((x * TWO_OVER_PI) + (1i128 << 63)) >> 64;
    let r = x - k * HALF_PI;
    let r2 = (r * r) >> 32;
    let horner = |c: &[i128]| c.iter().rev().fold(0i128, |acc, &k| ((acc * r2) >> 32) + k);
    let sin = (horner(&S) * r) >> 32;
    let cos = horner(&C);
    let (sin, cos) = match k.rem_euclid(4) {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    };
    (Fx::from_bits(sin as i64), Fx::from_bits(cos as i64))
}

/// Rotation quaternion `{x, y, z, w}` (moho's Lua layout), Y-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FxQuat {
    pub x: Fx,
    pub y: Fx,
    pub z: Fx,
    pub w: Fx,
}

impl Default for FxQuat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl FxQuat {
    pub const IDENTITY: Self = Self { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ZERO, w: Fx::ONE };

    /// Inverse rotation (for a unit quaternion).
    pub fn conjugate(self) -> Self {
        Self { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, o: Self) -> Self {
        Self {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }

    /// Heading (about +Y, up), pitch (about +X), roll (about +Z), radians,
    /// composed as Y·X·Z. Deterministic CORDIC trig.
    pub fn from_hpr(heading: Fx, pitch: Fx, roll: Fx) -> Self {
        let half = |a: Fx| sin_cos(a / 2);
        let (sh, ch) = half(heading);
        let (sp, cp) = half(pitch);
        let (sr, cr) = half(roll);
        let qy = Self { x: Fx::ZERO, y: sh, z: Fx::ZERO, w: ch };
        let qx = Self { x: sp, y: Fx::ZERO, z: Fx::ZERO, w: cp };
        let qz = Self { x: Fx::ZERO, y: Fx::ZERO, z: sr, w: cr };
        qy.mul(qx).mul(qz)
    }

    /// Pure heading rotation (about +Y) and its forward vector `(sin h, cos h)`
    /// on XZ, from one CORDIC call (double-angle identities). Matches
    /// `from_hpr(h, 0, 0)`.
    pub fn from_heading(h: Fx) -> (Self, Fx, Fx) {
        let (s, c) = sin_cos(h / 2);
        let q = Self { x: Fx::ZERO, y: s, z: Fx::ZERO, w: c };
        (q, (s * c) * 2, c * c - s * s)
    }

    /// Rotation turning +Z (moho's forward) to face unit vector `dir`, with no
    /// roll: yaw · pitch, each built from its half-vector (the quaternion
    /// taking `a` to `b` is `normalize(a × b, 1 + a·b)`), so no trig — 3
    /// square roots, 2 divisions. Deterministic (integer sqrt).
    pub fn look_along(dir: FxVec3) -> Self {
        let horizontal = sqrt(dir.x * dir.x + dir.z * dir.z);
        // Yaw: +Z -> (dx, 0, dz)/h; scale-free, so h needn't be divided out.
        let yaw = match Self::normalize(Fx::ZERO, dir.x, Fx::ZERO, horizontal + dir.z) {
            Some(q) => q,
            // Facing exactly -Z: half a turn. Straight up/down: no heading.
            None if horizontal > Fx::ZERO => Self { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO, w: Fx::ZERO },
            None => Self::IDENTITY,
        };
        // Pitch about +X: +Z -> (0, dy, h). Positive pitch tips +Z down.
        let pitch = Self::normalize(-dir.y, Fx::ZERO, Fx::ZERO, Fx::ONE + horizontal).unwrap_or(Self::IDENTITY);
        yaw.mul(pitch)
    }

    /// The shortest rotation taking unit vector `a` to unit vector `b`
    /// (`normalize(a × b, 1 + a·b)`); half a turn about +Y when they are
    /// opposite.
    pub fn between(a: FxVec3, b: FxVec3) -> Self {
        let (x, y, z) = (a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
        Self::normalize(x, y, z, Fx::ONE + a.x * b.x + a.y * b.y + a.z * b.z).unwrap_or(Self { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO, w: Fx::ZERO })
    }

    /// This rotation scaled back to unit length (fixed-point products drift
    /// when rotations are chained tick after tick); identity if degenerate.
    pub fn normalized(self) -> Self {
        Self::normalize(self.x, self.y, self.z, self.w).unwrap_or(Self::IDENTITY)
    }

    /// The rotation about `v`'s direction by `|v|` radians (a rotation
    /// vector: angular velocity x time).
    pub fn from_rotation_vector(v: FxVec3) -> Self {
        let angle = v.length();
        if angle == Fx::ZERO {
            return Self::IDENTITY;
        }
        let (s, c) = sin_cos(angle / 2);
        let k = s / angle;
        Self { x: v.x * k, y: v.y * k, z: v.z * k, w: c }
    }

    pub fn normalize(x: Fx, y: Fx, z: Fx, w: Fx) -> Option<Self> {
        let len = sqrt(x * x + y * y + z * z + w * w);
        (len > Fx::ZERO).then(|| {
            let inv = Fx::ONE / len;
            Self { x: x * inv, y: y * inv, z: z * inv, w: w * inv }
        })
    }

    pub fn to_f64(self) -> (f64, f64, f64, f64) {
        (self.x.to_num(), self.y.to_num(), self.z.to_num(), self.w.to_num())
    }

    /// Rotates `v` by this (unit) quaternion: v' = q v q*.
    pub fn rotate(self, v: FxVec3) -> FxVec3 {
        let two = Fx::from_num(2);
        // t = 2 * cross(q.xyz, v)
        let tx = two * (self.y * v.z - self.z * v.y);
        let ty = two * (self.z * v.x - self.x * v.z);
        let tz = two * (self.x * v.y - self.y * v.x);
        // v' = v + w*t + cross(q.xyz, t)
        FxVec3 {
            x: v.x + self.w * tx + (self.y * tz - self.z * ty),
            y: v.y + self.w * ty + (self.z * tx - self.x * tz),
            z: v.z + self.w * tz + (self.x * ty - self.y * tx),
        }
    }

    /// The rotation taking the local X / Y / Z axes to `x` / `y` / `z` (an
    /// orthonormal basis, e.g. an `.scmap` prop's rotation rows). Shepperd's
    /// method: branch on the largest diagonal term so the square root never
    /// sees a tiny argument. All fixed point.
    pub fn from_axes(x: FxVec3, y: FxVec3, z: FxVec3) -> Self {
        // R's columns are the axis images: R[i][j] = axis_j[i].
        let (r00, r01, r02) = (x.x, y.x, z.x);
        let (r10, r11, r12) = (x.y, y.y, z.y);
        let (r20, r21, r22) = (x.z, y.z, z.z);
        let (one, two, four) = (Fx::ONE, Fx::from_num(2), Fx::from_num(4));
        let trace = r00 + r11 + r22;
        let q = if trace > Fx::ZERO {
            let s = sqrt(trace + one) * two;
            Self { w: s / four, x: (r21 - r12) / s, y: (r02 - r20) / s, z: (r10 - r01) / s }
        } else if r00 > r11 && r00 > r22 {
            let s = sqrt(one + r00 - r11 - r22) * two;
            Self { w: (r21 - r12) / s, x: s / four, y: (r01 + r10) / s, z: (r02 + r20) / s }
        } else if r11 > r22 {
            let s = sqrt(one + r11 - r00 - r22) * two;
            Self { w: (r02 - r20) / s, x: (r01 + r10) / s, y: s / four, z: (r12 + r21) / s }
        } else {
            let s = sqrt(one + r22 - r00 - r11) * two;
            Self { w: (r10 - r01) / s, x: (r02 + r20) / s, y: (r12 + r21) / s, z: s / four }
        };
        // Same rotation either sign; keep w >= 0 so results are canonical.
        if q.w < Fx::ZERO { Self { x: -q.x, y: -q.y, z: -q.z, w: -q.w } } else { q }
    }

    /// Heading on the XZ plane (radians): direction of the rotated +Z axis,
    /// `atan2(fwd.x, fwd.z)`; 0 = facing +Z (moho "south").
    pub fn heading(self) -> Fx {
        let two = Fx::from_num(2);
        let fx = two * (self.x * self.z + self.w * self.y);
        let fz = Fx::ONE - two * (self.x * self.x + self.y * self.y);
        atan2(fx, fz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integer sin_cos is within 2e-9 of f64's over many turns, and exact
    /// at zero.
    #[test]
    fn sin_cos_accuracy() {
        let mut worst = 0.0f64;
        for i in -200_000i64..200_000 {
            let a = Fx::from_num(i as f64 * 0.000_157_3);
            let (s, c) = sin_cos(a);
            let f = a.to_num::<f64>();
            worst = worst.max((s.to_num::<f64>() - f.sin()).abs()).max((c.to_num::<f64>() - f.cos()).abs());
        }
        assert!(worst < 2e-9, "worst error {worst}");
        assert_eq!(sin_cos(Fx::ZERO), (Fx::ZERO, Fx::ONE));
    }

    /// The integer atan2 is within 5e-8 rad of f64's everywhere (scales from
    /// 1e-4 to 1e5), exact on the axes, and the same as before on them.
    #[test]
    fn atan2_accuracy() {
        let mut worst = 0.0f64;
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        for i in 0..200_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let scale = [1e-4, 0.01, 1.0, 100.0, 1e4, 1e5][i % 6];
            let fy = ((seed & 0xffff) as f64 / 32768.0 - 1.0) * scale;
            let fx = (((seed >> 16) & 0xffff) as f64 / 32768.0 - 1.0) * scale;
            let (y, x) = (Fx::from_num(fy), Fx::from_num(fx));
            if y == Fx::ZERO && x == Fx::ZERO {
                continue;
            }
            let got = atan2(y, x).to_num::<f64>();
            let want = y.to_num::<f64>().atan2(x.to_num::<f64>());
            worst = worst.max((got - want).abs());
        }
        assert!(worst < 5e-8, "worst error {worst}");
        let (one, zero) = (Fx::ONE, Fx::ZERO);
        assert_eq!(atan2(zero, one), zero);
        assert_eq!(atan2(zero, zero), zero);
        assert_eq!(atan2(zero, -one), Fx::PI);
        assert_eq!(atan2(one, zero), Fx::from_bits(Fx::PI.to_bits() / 2));
        assert_eq!(atan2(-one, zero), -Fx::from_bits(Fx::PI.to_bits() / 2));
    }

    #[test]
    fn round_trips_map_coordinates() {
        let v = FxVec3::from_f64(512.25, 17.5, 1023.0);
        assert_eq!(v.to_f64(), (512.25, 17.5, 1023.0));
        // Non-dyadic values round deterministically to the nearest 2^-32.
        let w = FxVec2::from_f64(0.1, -0.1);
        assert_eq!(w.x, -w.z);
    }

    #[test]
    fn sqrt_matches_fixed_exactly() {
        let mut edge = vec![Fx::DELTA, Fx::ONE, Fx::MAX, Fx::from_num(2), Fx::from_bits(3), Fx::from_num(4096)];
        // Perfect squares and their neighbours, where a seed is most likely off by one.
        for k in [1i64, 3, 1 << 20, (1 << 31) - 1, 3_037_000_499] {
            let sq = Fx::from_bits(((k as i128 * k as i128) >> 32) as i64);
            edge.extend([sq, sq + Fx::DELTA, sq.saturating_sub(Fx::DELTA)]);
        }
        let mut rng = crate::engine::sim::rng::SimRng::new(9);
        edge.extend((0..20_000).map(|i| Fx::from_bits((rng.next_u64() >> (i % 40)) as i64 & i64::MAX)));
        for v in edge.into_iter().filter(|&v| v >= Fx::ZERO) {
            assert_eq!(sqrt(v), v.sqrt(), "sqrt({v:?})");
        }
        assert_eq!(sqrt(Fx::ZERO), Fx::ZERO);
        assert_eq!(sqrt(-Fx::ONE), Fx::ZERO);
    }

    #[test]
    fn from_heading_matches_hpr() {
        for h in [-3.0, -1.2, 0.0, 0.4, 2.9] {
            let h = Fx::from_num(h);
            let (q, s, c) = FxQuat::from_heading(h);
            assert_eq!(q, FxQuat::from_hpr(h, Fx::ZERO, Fx::ZERO));
            let (rs, rc) = sin_cos(h);
            assert!((s - rs).abs() < Fx::from_num(1e-8) && (c - rc).abs() < Fx::from_num(1e-8));
        }
    }

    #[test]
    fn heading_quaternion() {
        let q = FxQuat::from_hpr(Fx::PI, Fx::ZERO, Fx::ZERO);
        let (x, y, z, w) = q.to_f64();
        assert!(x.abs() < 1e-6 && z.abs() < 1e-6);
        assert!((y - 1.0).abs() < 1e-6 && w.abs() < 1e-6);
        assert_eq!(FxQuat::from_hpr(Fx::ZERO, Fx::ZERO, Fx::ZERO), FxQuat::IDENTITY);
        assert_eq!(FxQuat::IDENTITY.heading(), Fx::ZERO);
        let q90 = FxQuat::from_hpr(Fx::FRAC_PI_2, Fx::ZERO, Fx::ZERO);
        let (x, _, z) = q90.rotate(FxVec3::from_f64(0.0, 0.0, 1.0)).to_f64();
        assert!((x - 1.0).abs() < 1e-6 && z.abs() < 1e-6);
        let up = FxVec3::from_f64(0.6, 0.8, 0.0);
        let (x, y, z) = FxQuat::look_along(up).rotate(FxVec3::from_f64(0.0, 0.0, 1.0)).to_f64();
        assert!((x - 0.6).abs() < 1e-6 && (y - 0.8).abs() < 1e-6 && z.abs() < 1e-6, "{x} {y} {z}");
        // Matches the trig construction (yaw, then pitch) and has no roll.
        for (dx, dy, dz) in [(0.3, -0.4, -0.866), (-0.48, 0.6, 0.64), (0.0, 0.0, -1.0), (0.0, 1.0, 0.0), (1.0, 0.0, 0.0)] {
            let d = FxVec3::from_f64(dx, dy, dz).normalized().unwrap();
            let q = FxQuat::look_along(d);
            let (fx, fy, fz) = q.rotate(FxVec3::from_f64(0.0, 0.0, 1.0)).to_f64();
            let (ex, ey, ez) = d.to_f64();
            assert!((fx - ex).abs() < 1e-6 && (fy - ey).abs() < 1e-6 && (fz - ez).abs() < 1e-6, "{d:?}");
            let right = q.rotate(FxVec3::from_f64(1.0, 0.0, 0.0)).to_f64();
            assert!(right.1.abs() < 1e-6, "no roll: right axis stays level for {d:?}");
        }
        let h = FxQuat::from_hpr(Fx::from_num(1.25), Fx::ZERO, Fx::ZERO).heading();
        assert!((h.to_num::<f64>() - 1.25).abs() < 1e-6);
    }

    #[test]
    fn from_axes_round_trips_rotations() {
        let near = |a: Fx, b: Fx| (a - b).abs() < Fx::from_num(0.0005);
        let unit = |x: f64, y: f64, z: f64| FxVec3::from_f64(x, y, z);
        for (h, p, r) in [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (-2.5, 0.0, 0.0), (3.1, 0.0, 0.0), (0.7, 0.4, -0.3), (2.9, -1.2, 0.8)] {
            let q = FxQuat::from_hpr(Fx::from_num(h), Fx::from_num(p), Fx::from_num(r));
            let q = if q.w < Fx::ZERO { FxQuat { x: -q.x, y: -q.y, z: -q.z, w: -q.w } } else { q };
            let back = FxQuat::from_axes(q.rotate(unit(1.0, 0.0, 0.0)), q.rotate(unit(0.0, 1.0, 0.0)), q.rotate(unit(0.0, 0.0, 1.0)));
            assert!(near(back.x, q.x) && near(back.y, q.y) && near(back.z, q.z) && near(back.w, q.w), "hpr {h},{p},{r}: {q:?} vs {back:?}");
        }
    }
}
