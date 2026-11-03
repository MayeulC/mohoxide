table.insert(options.ui.items, {
    title = 'Popup: ACU Below 10%', key = 'pip_acu_low_health', type = 'toggle', default = 1,
    custom = { states = { { text = '<LOC _Off>', key = 0 }, { text = '<LOC _On>', key = 1 } } },
})
