version = 3
ScenarioInfo = {
    name = 'Audit Flat',
    description = 'Flat 81 km weapon-audit map: alternating 512-unit land / water bands across x, grid texture (dev/audit/flat_map.py).',
    type = 'skirmish',
    starts = true,
    preview = '',
    size = {4096, 4096},
    map = '/maps/audit_flat/audit_flat.scmap',
    save = '/maps/audit_flat/audit_flat_save.lua',
    script = '/maps/audit_flat/audit_flat_script.lua',
    norushradius = 0.000000,
    Configurations = {
        ['standard'] = {
            teams = {
                { name = 'FFA', armies = {'ARMY_1','ARMY_2',} },
            },
            customprops = {
            },
        },
    }}
