// FA's sky dome (`render/sky.rs`; moho's `SkyDome`, `effects/sky.fx`): the
// atmosphere (horizon colour to zenith colour through the horizon lookup),
// the planets (billboards of the sky's albedo, with glow) and the scrolling
// cirrus layers. Drawn first, without depth (sky.fx's `ZEnable = false`).
import package::globals::g;

struct Sky {
    // horizonBegin, horizonEnd, decalGlowMultiplier, cirrusMultiplier.
    horizon: vec4<f32>,
    horizon_color: vec4<f32>,
    zenith_color: vec4<f32>,
    cirrus_color: vec4<f32>,
    // Per cirrus layer: frequency xy, speed, 0; then direction xy, 0, 0.
    cirrus: array<vec4<f32>, 8>,
};
@group(1) @binding(0) var<uniform> sky: Sky;
@group(1) @binding(1) var lookup_tex: texture_2d<f32>;
@group(1) @binding(2) var cirrus_tex: texture_2d<f32>;
@group(1) @binding(3) var albedo_tex: texture_2d<f32>;
@group(1) @binding(4) var glow_tex: texture_2d<f32>;
@group(1) @binding(5) var point_smp: sampler;
@group(1) @binding(6) var clamp_smp: sampler;
@group(1) @binding(7) var wrap_smp: sampler;

const TWO_PI: f32 = 6.283185;

// A sky point's clip position, never clipped by the far plane (the dome is
// thousands of units out); depth isn't tested.
fn sky_clip(world: vec3<f32>) -> vec4<f32> {
    let c = g.view_proj * vec4<f32>(world, 1.0);
    return vec4<f32>(c.xy, 0.5 * c.w, c.w);
}

// sky.fx `computeCirrusCoord`: the layer's direction rotates the ground
// position, which then scrolls along it.
fn cirrus_coord(time: f32, p: vec2<f32>, layer: u32) -> vec2<f32> {
    let a = sky.cirrus[2u * layer];
    let d = normalize(sky.cirrus[2u * layer + 1u].xy + vec2<f32>(1e-6, 0.0));
    let rotated = vec2<f32>(p.x * d.x + p.y * d.y, p.x * d.y - p.y * d.x);
    return a.xy * (rotated - time * a.z * d);
}

struct DomeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) elevation: f32,
    @location(1) theta: f32,
    @location(2) c01: vec4<f32>,
    @location(3) c23: vec4<f32>,
};

@vertex fn vs_dome(@location(0) position: vec3<f32>, @location(1) theta: f32) -> DomeOut {
    // In ticks, as moho's `tick + interpolant`.
    let time = g.params.z * 10.0;
    var o: DomeOut;
    o.clip = sky_clip(position);
    o.elevation = position.y;
    o.theta = theta;
    o.c01 = vec4<f32>(cirrus_coord(time, position.xz, 0u), cirrus_coord(time, position.xz, 1u));
    o.c23 = vec4<f32>(cirrus_coord(time, position.xz, 2u), cirrus_coord(time, position.xz, 3u));
    return o;
}

struct SceneOut {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
};

// sky.fx `AtmospherePS`.
@fragment fn fs_atmosphere(i: DomeOut) -> SceneOut {
    let th = clamp(i.theta, 0.0, TWO_PI) / TWO_PI;
    let tv = clamp((i.elevation - sky.horizon.x) / (sky.horizon.y - sky.horizon.x), 0.0, 1.0);
    let t = textureSampleLevel(lookup_tex, point_smp, vec2<f32>(th, 0.25), 0.0).a * textureSampleLevel(lookup_tex, point_smp, vec2<f32>(tv, 0.75), 0.0).a;
    var o: SceneOut;
    o.color = vec4<f32>(mix(sky.horizon_color.rgb, sky.zenith_color.rgb, 1.0 - t), 1.0);
    o.glow = vec4<f32>(0.0);
    return o;
}

// sky.fx `CirrusPS`.
@fragment fn fs_cirrus(i: DomeOut) -> SceneOut {
    let c0 = textureSample(cirrus_tex, wrap_smp, i.c01.xy).r;
    let c1 = textureSample(cirrus_tex, wrap_smp, i.c01.zw).g;
    let c2 = textureSample(cirrus_tex, wrap_smp, i.c23.xy).b;
    let c3 = textureSample(cirrus_tex, wrap_smp, i.c23.zw).a;
    var o: SceneOut;
    o.color = vec4<f32>(sky.cirrus_color.rgb, sky.horizon.w * c0 * c1 * c2 * c3);
    o.glow = vec4<f32>(0.0);
    return o;
}

struct PlanetOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// sky.fx `DecalVS`: a billboard facing the camera, rotated, showing part of
// the albedo.
@vertex fn vs_planet(
    @builtin(vertex_index) v: u32,
    @location(0) position: vec4<f32>,
    @location(1) size: vec2<f32>,
    @location(2) region: vec4<f32>,
) -> PlanetOut {
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let corner = corners[v];
    var o: PlanetOut;
    o.uv = region.xy + 0.5 * region.zw * (corner + vec2<f32>(1.0));
    o.uv.y = 1.0 - o.uv.y;
    let c = size * corner;
    let s = sin(position.w);
    let k = cos(position.w);
    let r = vec2<f32>(c.x * k - c.y * s, c.x * s + c.y * k);
    let forward = g.forward.xyz;
    let right = normalize(cross(forward, vec3<f32>(0.0, 1.0, 0.0)));
    let up = cross(right, forward);
    o.clip = sky_clip(position.xyz + r.x * right + r.y * up);
    return o;
}

// sky.fx `DecalAlbedoPS` (blended over the frame).
@fragment fn fs_planet(i: PlanetOut) -> SceneOut {
    var o: SceneOut;
    o.color = textureSample(albedo_tex, clamp_smp, i.uv);
    o.glow = vec4<f32>(0.0);
    return o;
}

// sky.fx `DecalGlowPS` (into the glow only).
@fragment fn fs_planet_glow(i: PlanetOut) -> SceneOut {
    var o: SceneOut;
    o.color = vec4<f32>(0.0);
    o.glow = vec4<f32>(sky.horizon.z * textureSample(glow_tex, clamp_smp, i.uv).a, 0.0, 0.0, 0.0);
    return o;
}
