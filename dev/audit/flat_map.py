#!/usr/bin/env python3
"""Generates the weapon audit's flat test maps.

`audit_duel` (20 km, 1024 units): the census map. Every audit case runs in
a session of its own with its engagement midpoint at the map centre
(src/weapon_audit.rs), so the map holds one case at a time: the south half
(z < 552) is all land, where a land-to-land case (or a land unit against an
air target) sits exactly at the centre; north of it a water band (x 256 to
896, land either side) holds the naval and shore-crossing cases, each as
near the centre as its media allow. A small map keeps each session's
start-up (the navigation grids are built over the whole map) short.

`audit_flat` (81 km, 4096 units): eight 512-unit bands across x, land and
water alternating (land first), so every audit row (the audit's strips run
along +x) crosses four shores each way. It holds many cases at once: the
script route's multi-case layout (WEAPON_AUDIT_IDS), for watching several
engagements together; those rows are not the census's.

The land is perfectly flat, 2 units above the water; the seabed drops on an
8-unit ramp centred on each shore, so nothing on either map blocks line of
sight above the water.

Lighting, water shading and the sky box are copied from SCMP_030 (an 81 km
map); the heightmap, strata (one grid albedo), decals, masks, terrain types
and props are replaced.

    python3 dev/audit/flat_map.py [game dir] [out maps dir]

Defaults: ../Supreme Commander Forged Alliance and tests/vault/maps (the
copy in the repo, which the audit mounts: --audit-table, --audit-test and
tests/weapon_audit.rs). Any other session can use them with
    cargo run --release -- --vault tests/vault --map /maps/audit_duel/audit_duel_scenario.lua
"""

import os
import struct
import sys
from compression import zstd  # Python 3.14+

LAND = 52.0  # 2 units above the water: a shore is no sight-blocking ridge
SEABED = 20.0
WATER = (50.0, 45.0, 25.0)  # surface, deep, abyss
BAND = 512  # audit_flat: odd bands are water
RAMP = 8
HEIGHT_SCALE = 1.0 / 128.0
GRID_TEX = 512  # texture pixels; 4 grid cells per repeat
GRID_REPEAT = 256.0  # world units per texture repeat -> a line every 64 units


class Reader:
    def __init__(self, b):
        self.b, self.o = b, 0

    def take(self, n):
        s = self.b[self.o:self.o + n]
        self.o += n
        return s

    def i32(self):
        return struct.unpack('<i', self.take(4))[0]

    def u8(self):
        return self.take(1)[0]

    def cstr(self):
        end = self.b.index(b'\0', self.o)
        s = self.b[self.o:end]
        self.o = end + 1
        return s

    def floats(self, n):
        self.take(4 * n)

    def blob(self):
        self.take(self.i32())


def template_sections(b):
    """Byte ranges of SCMP_030 to keep: render header up to the water
    levels, the water shading after them, minimap colours, and the sky box."""
    r = Reader(b)
    r.take(30)
    r.blob()  # preview
    version = r.i32()
    assert version == 60, version
    hw, hh = r.i32(), r.i32()
    r.take(4 + 2 * (hw + 1) * (hh + 1))
    render_start = r.o
    r.u8()
    for _ in range(3):
        r.cstr()
    for _ in range(r.i32()):
        r.cstr()
        r.cstr()
    r.floats(23)  # lighting
    render_end = r.o
    r.u8()
    r.floats(3)  # water levels
    shading_start = r.o
    r.floats(20)
    r.cstr()
    r.cstr()
    r.floats(4)
    for _ in range(4):
        r.floats(2)
        r.cstr()
    shading_end = r.o
    for _ in range(r.i32()):  # wave generators (dropped)
        r.cstr()
        r.cstr()
        r.floats(17)
    minimap = r.take(28)
    for _ in range(19):
        r.cstr()
        r.floats(1)
    r.take(8)
    for _ in range(r.i32()):
        r.take(8)
        for _ in range(r.i32()):
            r.blob()
        r.take(4 * 11 + 4)
    for _ in range(r.i32()):
        r.take(4)
        r.cstr()
        r.take(4 * r.i32())
    r.take(8)
    for _ in range(r.i32()):
        r.blob()
    r.blob()
    r.blob()
    for _ in range(r.i32()):
        r.blob()
    r.take(3 * (hw // 2) * (hh // 2) + hw * hh)
    sky_start = r.o
    r.floats(6)
    r.take(8)
    r.floats(8)
    r.cstr()
    r.cstr()
    for _ in range(r.i32()):
        r.floats(10)
    r.take(3)
    r.floats(4)
    r.cstr()
    for _ in range(r.i32()):
        r.floats(5)
    r.floats(1)
    sky = b[sky_start:r.o]

    return b[render_start:render_end], b[shading_start:shading_end], minimap, sky


def dds(levels):
    """Uncompressed 32-bit BGRA DDS from a list of (size, bytes) mip levels."""
    size = levels[0][0]
    header = struct.pack(
        '<4sIIIIIII44xIIIIIIIIIIIII',
        b'DDS ', 124, 0x21007, size, size, size * 4, 0, len(levels),
        32, 0x41, 0, 32, 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000,
        0x401008, 0, 0, 0, 0)
    return header + b''.join(data for _, data in levels)


def grid_texture():
    levels = []
    size = GRID_TEX
    while size >= 1:
        cell = max(size // 4, 1)
        line = max(2 * size // GRID_TEX, 1)
        light, dark = bytes((150, 150, 150, 0)), bytes((70, 70, 70, 0))
        line_row = dark * size
        cell_row = b''.join(dark if x % cell < line else light for x in range(size))
        data = b''.join(line_row if z % cell < line else cell_row for z in range(size))
        levels.append((size, data))
        size //= 2
    return dds(levels)


class Flat:
    """audit_flat: 512-unit bands across x, odd ones water."""
    name = 'audit_flat'
    size = 4096
    description = 'Flat 81 km weapon-audit map: alternating 512-unit land / water bands across x, grid texture (dev/audit/flat_map.py).'
    starts = ((256.5, 2048.5), (3328.5, 2048.5))
    z_free = True

    def wet(self, x, z):
        return int(x // BAND) % 2 == 1

    def into(self, x, z):
        # distance to the nearest shore (band edges inside the map), signed
        # positive into the water
        shore = min(abs(x - k * BAND) for k in range(1, self.size // BAND))
        return shore if self.wet(min(x, self.size - 1), z) else -shore


class Duel:
    """audit_duel: land south of z 552; north of it water from x 256 to 896."""
    name = 'audit_duel'
    size = 1024
    description = 'Flat 20 km weapon-audit map: land around the centre, a water band to the north with land either side, grid texture (dev/audit/flat_map.py).'
    starts = ((128.5, 128.5), (896.5, 128.5))
    z_free = False
    # Row 0 of a centred case (z 512, up to a 72-unit footprint) stays south
    # of the shore ramp; the next row (z >= 562) north of it.
    SHORE_Z = 552
    WATER_X = (256, 896)

    def wet(self, x, z):
        return z >= self.SHORE_Z and self.WATER_X[0] <= x < self.WATER_X[1]

    def into(self, x, z):
        lo, hi = self.WATER_X
        if self.wet(x, z):
            return min(z - self.SHORE_Z, x - lo, hi - x)
        dx = max(lo - x, 0, x - hi)
        dz = max(self.SHORE_Z - z, 0)
        return -(dx * dx + dz * dz) ** 0.5


def ground(m, x, z):
    # the ramp is centred on the shore
    depth = min(max((m.into(x, z) + RAMP / 2) / RAMP, 0.0), 1.0)
    return LAND + (SEABED - LAND) * depth


def water_map(m):
    """The renderer's water tint reads its depth from the green channel:
    0 on land, 1 at the abyss level."""
    size = 512
    surface, _, abyss = WATER
    rows = []
    for z in range(size):
        texel = b''
        for x in range(size):
            depth = (surface - ground(m, (x + 0.5) * m.size / size, (z + 0.5) * m.size / size)) / (surface - abyss)
            texel += bytes((0, round(min(max(depth, 0.0), 1.0) * 255), 0, 255))
        rows.append(texel)
    return dds([(size, b''.join(rows))])


def preview(m):
    size = 256
    land, water = bytes((90, 120, 110, 255)), bytes((140, 90, 40, 255))
    rows = []
    for z in range(size):
        rows.append(b''.join(water if m.wet((x + 0.5) * m.size / size, (z + 0.5) * m.size / size) else land
                             for x in range(size)))
    return dds([(size, b''.join(rows))])


def scmap(m, template):
    render, shading, minimap, sky = template_sections(template)
    out = bytearray()
    SIZE, NAME = m.size, m.name
    p = preview(m)
    out += b'Map\x1a' + struct.pack('<iIiffih', 2, 0xBEEFFEED, 2, SIZE, SIZE, 0, 0)
    out += struct.pack('<i', len(p)) + p
    out += struct.pack('<iiif', 60, SIZE, SIZE, HEIGHT_SCALE)
    for z in range(SIZE + 1):
        if z == 0 or not m.z_free:
            row = struct.pack('<%dH' % (SIZE + 1), *(round(ground(m, x, z) / HEIGHT_SCALE) for x in range(SIZE + 1)))
        out += row
    out += render
    out += b'\x01' + struct.pack('<3f', *WATER) + shading
    out += struct.pack('<i', 0)  # wave generators
    out += minimap
    grid = f'/maps/{NAME}/grid.dds'.encode()
    albedo = [(grid, GRID_REPEAT)] + [(b'', 4.0)] * 9
    normals = [(b'', 4.0)] * 9
    for path, scale in albedo + normals:
        out += path + b'\0' + struct.pack('<f', scale)
    out += struct.pack('<ii', 0, 0)
    out += struct.pack('<ii', 0, 0)  # decals, decal groups
    out += struct.pack('<ii', SIZE, SIZE)
    water = water_map(m)
    out += struct.pack('<iii', 0, 0, 0)  # normal maps, mask low, mask high
    out += struct.pack('<ii', 1, len(water)) + water
    out += bytes(3 * (SIZE // 2) * (SIZE // 2))
    out += bytes((1,)) * (SIZE * SIZE)  # terrain type: Default
    out += sky
    out += struct.pack('<i', 0)  # props
    return bytes(out)


def scenario(m):
    SIZE, NAME = m.size, m.name
    title = ' '.join(w.capitalize() for w in NAME.split('_'))
    return f"""version = 3
ScenarioInfo = {{
    name = '{title}',
    description = '{m.description}',
    type = 'skirmish',
    starts = true,
    preview = '',
    size = {{{SIZE}, {SIZE}}},
    map = '/maps/{NAME}/{NAME}.scmap',
    save = '/maps/{NAME}/{NAME}_save.lua',
    script = '/maps/{NAME}/{NAME}_script.lua',
    norushradius = 0.000000,
    Configurations = {{
        ['standard'] = {{
            teams = {{
                {{ name = 'FFA', armies = {{'ARMY_1','ARMY_2',}} }},
            }},
            customprops = {{
            }},
        }},
    }}}}
"""

SCRIPT = """local ScenarioUtils = import('/lua/sim/ScenarioUtilities.lua')

function OnPopulate()
    ScenarioUtils.InitializeArmies()
end

function OnStart(self)
end
"""


def army(name):
    return f"""        ['{name}'] = {{
            personality = '',
            plans = '',
            color = 0,
            faction = 0,
            Economy = {{
                mass = 0,
                energy = 0,
            }},
            Alliances = {{
            }},
            ['Units'] = GROUP {{
                orders = '',
                platoon = '',
                Units = {{
                    ['INITIAL'] = GROUP {{
                        orders = '',
                        platoon = '',
                        Units = {{
                        }},
                    }},
                }},
            }},
            PlatoonBuilders = {{
                next_platoon_builder_id = '0',
                Builders = {{
                }},
            }},
        }},
"""


def marker(name, x, z):
    return f"""                ['{name}'] = {{
                    ['color'] = STRING( 'ff800080' ),
                    ['type'] = STRING( 'Blank Marker' ),
                    ['prop'] = STRING( '/env/common/props/markers/M_Blank_prop.bp' ),
                    ['orientation'] = VECTOR3( 0, 0, 0 ),
                    ['position'] = VECTOR3( {x}, {LAND}, {z} ),
                }},
"""


def save(m):
    (x1, z1), (x2, z2) = m.starts
    return f"""Scenario = {{
    next_area_id = '1',
    Props = {{
    }},
    Areas = {{
    }},
    MasterChain = {{
        ['_MASTERCHAIN_'] = {{
            Markers = {{
{marker('ARMY_1', x1, z1)}{marker('ARMY_2', x2, z2)}            }},
        }},
    }},
    Chains = {{
    }},
    next_queue_id = '1',
    Orders = {{
    }},
    next_platoon_id = '1',
    Platoons = {{
    }},
    next_army_id = '2',
    next_group_id = '0',
    next_unit_id = '0',
    Armies = {{
{army('ARMY_1')}{army('ARMY_2')}    }},
}}
"""


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    repos = os.path.normpath(os.path.join(here, '..', '..', '..'))
    game = sys.argv[1] if len(sys.argv) > 1 else os.path.join(repos, 'Supreme Commander Forged Alliance')
    maps = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, '..', '..', 'tests', 'vault', 'maps')
    with open(os.path.join(game, 'maps', 'SCMP_030', 'SCMP_030.scmap'), 'rb') as f:
        template = f.read()
    for m in (Duel(), Flat()):
        NAME = m.name
        out = os.path.join(maps, NAME)
        os.makedirs(out, exist_ok=True)
        # The big binaries are stored zstd-compressed (64 MB -> a few KB); the
        # engine's folder mounts serve `<name>.zst` decompressed as `<name>`.
        files = {
            f'{NAME}.scmap.zst': zstd.compress(scmap(m, template), 19),
            'grid.dds.zst': zstd.compress(grid_texture(), 19),
            f'{NAME}_scenario.lua': scenario(m).encode(),
            f'{NAME}_save.lua': save(m).encode(),
            f'{NAME}_script.lua': SCRIPT.encode(),
        }
        for name, data in files.items():
            with open(os.path.join(out, name), 'wb') as f:
                f.write(data)
        # an uncompressed copy beside the .zst would claim the same VFS path
        for stale in (f'{NAME}.scmap', 'grid.dds'):
            if os.path.exists(os.path.join(out, stale)):
                os.remove(os.path.join(out, stale))
        print(f'wrote {out}')


if __name__ == '__main__':
    main()
