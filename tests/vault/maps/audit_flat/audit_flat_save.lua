Scenario = {
    next_area_id = '1',
    Props = {
    },
    Areas = {
    },
    MasterChain = {
        ['_MASTERCHAIN_'] = {
            Markers = {
                ['ARMY_1'] = {
                    ['color'] = STRING( 'ff800080' ),
                    ['type'] = STRING( 'Blank Marker' ),
                    ['prop'] = STRING( '/env/common/props/markers/M_Blank_prop.bp' ),
                    ['orientation'] = VECTOR3( 0, 0, 0 ),
                    ['position'] = VECTOR3( 256.5, 52.0, 2048.5 ),
                },
                ['ARMY_2'] = {
                    ['color'] = STRING( 'ff800080' ),
                    ['type'] = STRING( 'Blank Marker' ),
                    ['prop'] = STRING( '/env/common/props/markers/M_Blank_prop.bp' ),
                    ['orientation'] = VECTOR3( 0, 0, 0 ),
                    ['position'] = VECTOR3( 3328.5, 52.0, 2048.5 ),
                },
            },
        },
    },
    Chains = {
    },
    next_queue_id = '1',
    Orders = {
    },
    next_platoon_id = '1',
    Platoons = {
    },
    next_army_id = '2',
    next_group_id = '0',
    next_unit_id = '0',
    Armies = {
        ['ARMY_1'] = {
            personality = '',
            plans = '',
            color = 0,
            faction = 0,
            Economy = {
                mass = 0,
                energy = 0,
            },
            Alliances = {
            },
            ['Units'] = GROUP {
                orders = '',
                platoon = '',
                Units = {
                    ['INITIAL'] = GROUP {
                        orders = '',
                        platoon = '',
                        Units = {
                        },
                    },
                },
            },
            PlatoonBuilders = {
                next_platoon_builder_id = '0',
                Builders = {
                },
            },
        },
        ['ARMY_2'] = {
            personality = '',
            plans = '',
            color = 0,
            faction = 0,
            Economy = {
                mass = 0,
                energy = 0,
            },
            Alliances = {
            },
            ['Units'] = GROUP {
                orders = '',
                platoon = '',
                Units = {
                    ['INITIAL'] = GROUP {
                        orders = '',
                        platoon = '',
                        Units = {
                        },
                    },
                },
            },
            PlatoonBuilders = {
                next_platoon_builder_id = '0',
                Builders = {
                },
            },
        },
    },
}
