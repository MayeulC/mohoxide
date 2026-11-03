// The scene's fallbacks (`render/scene.rs`): plain terrain, boxes, water.
import package::globals::{Globals, g, overlay_tex, overlay_smp, overlay_value, shadow_map, shadow_cmp, horizon_tex, environment_cube, brdf_lut, env_smp, sun_shadow, terrain_shadow, frame_rgb, fog_factor};

struct TerrainOut { @builtin(position) clip: vec4<f32>, @location(0) normal: vec3<f32>, @location(1) height: f32, @location(2) xz: vec2<f32> };

@vertex fn vs_terrain(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> TerrainOut {
    var o: TerrainOut;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.height = pos.y;
    o.xz = pos.xz;
    return o;
}

@fragment fn fs_terrain(i: TerrainOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let light = 0.35 + 0.65 * max(dot(n, -g.light_dir.xyz), 0.0);
    // Height colouring: sand near water, grass, rock on slopes, snow up high.
    let slope = 1.0 - n.y;
    var base = mix(vec3<f32>(0.33, 0.45, 0.22), vec3<f32>(0.45, 0.40, 0.33), smoothstep(0.15, 0.45, slope));
    base = mix(base, vec3<f32>(0.85, 0.85, 0.88), smoothstep(80.0, 140.0, i.height));
    let above_water = i.height - g.params.x;
    base = mix(vec3<f32>(0.62, 0.58, 0.42), base, smoothstep(0.0, 2.0, above_water));
    var c = base * light;
    if (g.overlay.x > 0.5) {
        let v = overlay_value(i.xz);
        if (g.overlay.y < 0.5) {
            // Nav grid: impassable cells tinted red.
            c = mix(c, vec3<f32>(0.9, 0.1, 0.1), (1.0 - v) * 0.55);
        } else {
            // Threat: transparent -> yellow -> red.
            let heat = mix(vec3<f32>(1.0, 0.9, 0.1), vec3<f32>(1.0, 0.1, 0.05), v);
            c = mix(c, heat, min(v * 1.5, 0.7) * step(0.001, v));
        }
    }
    return vec4<f32>(frame_rgb(c) * fog_factor(i.xz), 1.0);
}

struct BoxOut { @builtin(position) clip: vec4<f32>, @location(0) normal: vec3<f32>, @location(1) color: vec4<f32> };

@vertex fn vs_box(
    @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
    @location(2) m0: vec4<f32>, @location(3) m1: vec4<f32>, @location(4) m2: vec4<f32>, @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,
) -> BoxOut {
    let model = mat4x4<f32>(m0, m1, m2, m3);
    var o: BoxOut;
    o.clip = g.view_proj * model * vec4<f32>(pos, 1.0);
    o.normal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
    o.color = color;
    return o;
}

@fragment fn fs_box(i: BoxOut) -> @location(0) vec4<f32> {
    let light = 0.4 + 0.6 * max(dot(normalize(i.normal), -g.light_dir.xyz), 0.0);
    return vec4<f32>(frame_rgb(i.color.rgb * light), i.color.a);
}

struct WaterOut { @builtin(position) clip: vec4<f32> };
@vertex fn vs_water(@location(0) pos: vec3<f32>) -> WaterOut {
    var o: WaterOut;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    return o;
}
@fragment fn fs_water() -> @location(0) vec4<f32> {
    return vec4<f32>(frame_rgb(vec3<f32>(0.10, 0.28, 0.45)), 0.65);
}
