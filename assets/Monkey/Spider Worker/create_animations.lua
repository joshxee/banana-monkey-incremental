-- Run from repository root with ASEPRITE_BIN set to your Aseprite executable.
-- sprite-axi run "assets/Monkey/Spider Worker/create_animations.lua"
assert(loadfile("assets/Monkey/Spider Worker/create_idle.lua"))({animate=true})
