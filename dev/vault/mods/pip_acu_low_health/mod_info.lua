-- The PiP docs' example mod (docs/architecture/pip.md): a popup when an ACU drops below
-- 10% health. Try it: cargo run -- --session --vault dev/vault --mod 5e3c1f0a-pip-acu-low-health
name = "PiP: ACU below 10%"
uid = "5e3c1f0a-pip-acu-low-health"
version = 1
copyright = ""
description = "Opens a picture-in-picture popup when an ACU drops below 10% health."
author = "rts-core-engine"
url = ""
selectable = true
enabled = true
exclusive = false
ui_only = false
requires = {}
requiresNames = {}
conflicts = {}
before = {}
after = {}
