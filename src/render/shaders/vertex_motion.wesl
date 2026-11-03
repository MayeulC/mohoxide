// FA `mesh.fx`'s moving vertex shaders (`api::mesh::VertexMotion`, instance
// `bones.y`), shared by the models and their shadow casters. `time` is
// moho's shader `time`: game ticks the frame shows (`MeshRenderer::Batch`
// takes the tick and its fraction, `ConfigureShader` sets their sum), so
// the motion follows game speed and stops while the game is paused.

const STILL: u32 = 0u;
const UNDULATING: u32 = 1u;
const UNDULATING_CLUTTER: u32 = 2u;
const BLOATING: u32 = 3u;

// `windDirection`: mesh.fx's default (no map or code sets it).
const WIND_DIRECTION: vec3<f32> = vec3<f32>(0.707, 0.0, 0.707);

// Before the world transform: `BloatingNormalMappedVS` swells the mesh
// along the direction from its origin, phased by the instance's distance
// from the map corner.
fn motion_local(pos: vec3<f32>, motion: u32, translation: vec3<f32>, time: f32) -> vec3<f32> {
    if (motion != BLOATING || dot(pos, pos) < 1e-12) {
        return pos;
    }
    let direction = normalize(pos);
    return pos + 2.0 * length(direction.xz) * sin(0.1 * time + length(translation.xz)) * direction;
}

// After it: `UndulatingNormalMappedVS` / `UndulatingDepthVS` (trees) and
// `UndulatingClutterVS` lean each world vertex along the wind by its
// height in the mesh (`pos.y`, mesh units) times sin^2 of the time, phased
// by the instance's translation along the wind (neighbouring trees out of
// step).
fn motion_world(world: vec3<f32>, pos: vec3<f32>, motion: u32, translation: vec3<f32>, time: f32) -> vec3<f32> {
    var weight = 0.0;
    var rate = 0.0;
    if (motion == UNDULATING) {
        weight = 0.003 * pos.y;
        rate = 0.05;
    } else if (motion == UNDULATING_CLUTTER) {
        weight = 0.1 * pos.y;
        rate = 0.0625;
    } else {
        return world;
    }
    var s = sin(rate * time - dot(WIND_DIRECTION, translation));
    s = s * s;
    return world + weight * s * WIND_DIRECTION;
}
