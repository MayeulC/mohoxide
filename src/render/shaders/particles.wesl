// Particles, lights, beams and trails (`render/particles.rs`), a port of
// FA's `particle.fx`.
//
// Particles are records written once when they're born (`Particle`, FA's
// instanced vertex); the vertex shader evaluates everything from the
// record and the clock `pu.time` (sim ticks): position `p + v t + a t^2 / 2`
// (or the closed-form drag solution), size, rotation, the billboard
// (camera-facing, flat on the world XZ plane, or aligned along the
// direction of travel), the texture frame and strip, and the ramp
// coordinate (age over lifetime). Beams and trails are CPU ribbons
// (`vs_ribbon`).
import package::globals::{Globals, g, overlay_tex, overlay_smp, shadow_map, shadow_cmp, horizon_tex, environment_cube, brdf_lut, env_smp, sun_shadow, terrain_shadow, frame_rgb};

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var ramp: texture_2d<f32>;
@group(1) @binding(2) var smp: sampler;
@group(1) @binding(3) var ramp_smp: sampler;

struct ParticleUniforms {
    // x: the clock (sim ticks, fractional).
    time: vec4<f32>,
};

// One particle, as `CEfxEmitter::Emit` packs it (`WorldVS` inputs).
struct Particle {
    // Birth position; initial rotation (radians).
    pos_rot: vec4<f32>,
    // Velocity (ogrids/tick); rotation rate (radians/tick).
    vel_rate: vec4<f32>,
    // Acceleration (ogrids/tick^2); half-size at birth.
    accel_size: vec4<f32>,
    // Birth tick; lifetime (ticks); frame rate (frames/tick); half-size
    // growth per tick.
    time: vec4<f32>,
    // Frame width (1 / frame count); strip v offset; strip height; ramp row.
    tex: vec4<f32>,
    // Drag k, 1/k, 1/k^2; flags (bit cast).
    drag_flags: vec4<f32>,
};

@group(2) @binding(0) var background: texture_2d<f32>;
@group(2) @binding(1) var background_smp: sampler;
@group(2) @binding(2) var<uniform> pu: ParticleUniforms;
@group(2) @binding(3) var<storage, read> particles: array<Particle>;
// Instance -> record slot: a batch's records, wherever its segments sit in
// the pool, drawn as one run of instances.
@group(2) @binding(4) var<storage, read> indices: array<u32>;

// `Particle::drag_flags.w` bits.
const FLAT: u32 = 1u;
const ALIGN: u32 = 2u;
const NO_MOVE: u32 = 4u;
const DRAG: u32 = 8u;
const ANIMATED: u32 = 16u;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) ramp_uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

// Two triangles over the -1..1 quad (FA's corner stream).
fn corner_of(i: u32) -> vec2<f32> {
    switch i {
        case 0u: { return vec2<f32>(-1.0, -1.0); }
        case 1u: { return vec2<f32>(1.0, -1.0); }
        case 2u: { return vec2<f32>(1.0, 1.0); }
        case 3u: { return vec2<f32>(-1.0, -1.0); }
        case 4u: { return vec2<f32>(1.0, 1.0); }
        default: { return vec2<f32>(-1.0, 1.0); }
    }
}

fn safe_normalize(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    if l > 1e-6 {
        return v / l;
    }
    return fallback;
}

// The drawing view's camera axes (world space), from its globals: each
// world view draws the same particles facing its own camera.
fn cam_forward() -> vec3<f32> {
    return safe_normalize(g.forward.xyz, vec3<f32>(0.0, -1.0, 0.0));
}

fn cam_right() -> vec3<f32> {
    return safe_normalize(cross(cam_forward(), vec3<f32>(0.0, 1.0, 0.0)), vec3<f32>(1.0, 0.0, 0.0));
}

fn cam_up() -> vec3<f32> {
    return safe_normalize(cross(cam_right(), cam_forward()), vec3<f32>(0.0, 0.0, 1.0));
}

// `WorldVS` / `WorldVSAlign`: one particle per instance, six vertices.
@vertex fn vs_particle(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> Out {
    var o: Out;
    let p = particles[indices[ii]];
    let corner = corner_of(vi % 6u);
    let age = pu.time.x - p.time.x;
    let alpha = age / p.time.y;
    // Dead (or lifetime 0): nothing (every vertex at the origin's clip
    // position, a degenerate triangle).
    if alpha >= 1.0 || p.time.y <= 0.0 {
        o.clip = vec4<f32>(0.0, 0.0, -1.0, 0.0);
        return o;
    }
    // Born later this tick: FA draws these too, run backwards from their
    // birth (`WorldVS` only drops particles past their lifetime).
    let t = age;
    let flags = bitcast<u32>(p.drag_flags.w);
    let v = p.vel_rate.xyz;
    let a = p.accel_size.xyz;
    let drag = (flags & DRAG) != 0u;
    var pos = p.pos_rot.xyz;
    if (flags & NO_MOVE) == 0u {
        if drag {
            pos += (p.drag_flags.z * a - p.drag_flags.y * v) * (exp(-p.drag_flags.x * t) - 1.0) + p.drag_flags.y * a * t;
        } else {
            pos += v * t + 0.5 * a * t * t;
        }
    }
    let size = p.accel_size.w + p.time.w * t;
    if (flags & ALIGN) != 0u {
        // The quad's long side along the direction of travel (view-space
        // cross with the camera axis, as `WorldVSAlign`).
        var d: vec3<f32>;
        if drag {
            d = (v - p.drag_flags.y * a) * exp(-p.drag_flags.x * t) + p.drag_flags.y * a;
        } else {
            d = v + a * t;
        }
        let dir = safe_normalize(d, vec3<f32>(0.0, 1.0, 0.0));
        let x = safe_normalize(cross(cam_forward(), dir), cam_right());
        pos += x * (corner.x * size) + dir * (corner.y * size);
    } else {
        let rot = p.pos_rot.w + p.vel_rate.w * t;
        let c = cos(rot);
        let s = sin(rot);
        let rq = vec2<f32>(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
        if (flags & FLAT) != 0u {
            pos += (rq.x * vec3<f32>(1.0, 0.0, 0.0) + rq.y * vec3<f32>(0.0, 0.0, 1.0)) * size;
        } else {
            pos += (rq.x * cam_right() + rq.y * cam_up()) * size;
        }
    }
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    // Texture: FA maps corner +1 to v 1 (D3D's bottom row at the quad's
    // top); frames run along u (the sampler wraps u, clamps v).
    var uv = (corner + 1.0) * 0.5;
    if (flags & ANIMATED) != 0u {
        let frame = floor(p.time.z * t);
        uv.x = uv.x * p.tex.x + p.tex.x * frame;
        uv.y = uv.y * p.tex.z + p.tex.y;
    }
    o.uv = uv;
    o.ramp_uv = vec2<f32>(max(alpha, 0.0), p.tex.w);
    o.color = vec4<f32>(1.0);
    return o;
}

// A trail segment (`TrailVS`): the record holds the segment's two ends
// (with the time each was laid), the directions at each (the previous
// segment's and its own) and the texture v at each; the ribbon is
// extruded `width` to each side across the view. The ramp's u is the
// age over the fade length (the fragment drops what's outside 0..1, so a
// segment grows in over its tick and fades out at the tail), its v the
// side; the repeat texture runs along the trail in v.
@vertex fn vs_trail(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> Out {
    var o: Out;
    let p = particles[indices[ii]];
    let i = vi % 6u;
    // Triangles (P0-, P1-, P1+), (P0-, P1+, P0+).
    var at_end = 0.0;
    var side = -1.0;
    switch i {
        case 1u: { at_end = 1.0; }
        case 2u: { at_end = 1.0; side = 1.0; }
        case 4u: { at_end = 1.0; side = 1.0; }
        case 5u: { side = 1.0; }
        default: {}
    }
    let pos = mix(p.pos_rot.xyz, p.vel_rate.xyz, at_end);
    let laid = mix(p.pos_rot.w, p.vel_rate.w, at_end);
    let dir = mix(p.accel_size.xyz, p.time.xyz, at_end);
    let v = mix(p.accel_size.w, p.time.w, at_end);
    let across = safe_normalize(cross(cam_forward(), dir), cam_right());
    o.clip = g.view_proj * vec4<f32>(pos + across * (side * p.tex.x), 1.0);
    let s01 = side * 0.5 + 0.5;
    o.uv = vec2<f32>(s01, v + p.tex.z * pu.time.x);
    o.ramp_uv = vec2<f32>((pu.time.x - laid) / p.tex.y, s01);
    o.color = vec4<f32>(1.0);
    return o;
}

// Beams: CPU-built ribbons.
@vertex fn vs_ribbon(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) ramp_uv: vec2<f32>, @location(3) color: vec4<f32>) -> Out {
    var o: Out;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.uv = uv;
    o.ramp_uv = ramp_uv;
    o.color = color;
    return o;
}

// `WorldRefractPS`: the frame behind (copied before particles draw), its
// lookup nudged by the texture's red and green; alpha is the texture's
// times the ramp's.
@fragment fn fs_refract(i: Out) -> @location(0) vec4<f32> {
    let t = textureSample(tex, smp, i.uv);
    let ramp_alpha = textureSample(ramp, ramp_smp, i.ramp_uv).a;
    let offset = 0.005 * (2.0 * t.rg - 1.0);
    let screen = i.clip.xy / vec2<f32>(textureDimensions(background));
    let c = textureSample(background, background_smp, screen + offset).rgb;
    return vec4<f32>(c, t.a * ramp_alpha * i.color.a);
}

// FA's particle maths and blending are in gamma space, as the frame is.
fn shade(i: Out) -> vec4<f32> {
    // Trail segments outside their fade (`TrailPS`); particles' ramp u is
    // always within it.
    if i.ramp_uv.x < 0.0 || i.ramp_uv.x > 1.0 {
        discard;
    }
    let c = textureSample(tex, smp, i.uv) * textureSample(ramp, ramp_smp, i.ramp_uv) * i.color;
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
}

// `Write_RGB`: the glow is left alone.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    return shade(i);
}

struct GlowOut {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
};

// `Write_RGBA` (lights, beams): the alpha blends into the glow with the
// same factors as the colour, as FA's frame alpha did.
@fragment fn fs_glow(i: Out) -> GlowOut {
    let c = shade(i);
    return GlowOut(c, vec4<f32>(c.a, 0.0, 0.0, c.a));
}
