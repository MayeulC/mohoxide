version = 3
ScenarioInfo = {
    name = 'Audit Duel',
    description = 'Flat 20 km weapon-audit map: land around the centre, a water band to the north with land either side, grid texture (dev/audit/flat_map.py).',
    type = 'skirmish',
    starts = true,
    preview = '',
    size = {1024, 1024},
    map = '/maps/audit_duel/audit_duel.scmap',
    save = '/maps/audit_duel/audit_duel_save.lua',
    script = '/maps/audit_duel/audit_duel_script.lua',
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
