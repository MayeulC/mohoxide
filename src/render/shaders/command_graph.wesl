// The command graph (`render/command_graph.rs`): order lines, arrowheads,
// waypoints and selection brackets, FA's `primbatcher.fx` `TCommand` (texture times vertex colour)
// and `TCommandGlow` (the glow where the texture shows).
import package::globals::g;

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var smp: sampler;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) glow: f32,
};

@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>, @location(3) glow: f32) -> Out {
    var o: Out;
    o.clip = g.view_proj * vec4<f32>(pos, 1.0);
    o.uv = uv;
    o.color = color;
    o.glow = glow;
    return o;
}

// Selection brackets, one instance per selected unit (moho's corner
// pieces, `command_graph.rs` `bracket_pieces`): six vertices per corner,
// corners front-left, front-right, back-left, back-right; each the
// texture's quarter toward its corner on a square of half size `piece`
// centred on that corner of `half`, turned with the unit.
@vertex fn vs_bracket(@builtin(vertex_index) vi: u32, @location(0) centre_piece: vec4<f32>, @location(1) rotation: vec4<f32>, @location(2) half: vec4<f32>) -> Out {
    let k = vi / 6u;
    let quad = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let q = quad[vi % 6u];
    let u = f32(q & 1u);
    let v = f32(q >> 1u);
    let sx = select(-1.0, 1.0, (k & 1u) == 1u);
    let sz = select(1.0, -1.0, k >= 2u);
    let piece = centre_piece.w;
    let local = vec3<f32>(sx * half.x + (u - 0.5) * 2.0 * piece, 0.0, sz * half.y + (0.5 - v) * 2.0 * piece);
    // The unit's rotation (x, y, z, w) applied to the offset.
    let t = 2.0 * cross(rotation.xyz, local);
    let turned = local + rotation.w * t + cross(rotation.xyz, t);
    var o: Out;
    o.clip = g.view_proj * vec4<f32>(centre_piece.xyz + turned, 1.0);
    o.uv = vec2<f32>(select(0.5, 0.0, sx < 0.0) + 0.5 * u, select(0.5, 0.0, sz > 0.0) + 0.5 * v);
    o.color = vec4<f32>(1.0);
    o.glow = 0.0;
    return o;
}

struct Targets {
    @location(0) color: vec4<f32>,
    @location(1) glow: vec4<f32>,
};

@fragment fn fs(i: Out) -> Targets {
    let t = textureSample(tex, smp, i.uv);
    var o: Targets;
    o.color = t * i.color;
    // `CommandGlowPS`: the glow amount wherever the texture isn't clear.
    o.glow = vec4<f32>(select(0.0, i.glow, t.a > 0.0), 0.0, 0.0, 1.0);
    return o;
}
