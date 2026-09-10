-- Run from the repository root with ASEPRITE_BIN set:
-- sprite-axi run "assets/Monkey/Spider Worker/create_idle.lua"
-- sprite-axi export "assets/Monkey/Spider Worker/spider_monkey_idle.ase" png --out "assets/Monkey/Spider Worker/spider_monkey_idle.png"
-- Original pixel construction using the spider-monkey tutorial and sprite study.
local options = ... or {}
local spr = axi.new(64, 64, { mode = "rgb" })
spr:deleteLayer(spr.layers[1])
local ink, plum, light, skin, tan = "#3D3333", "#593E47", "#7A5859", "#734C44", "#BCAD9F"
axi.setPalette(options.palette or {ink, plum, light, skin, tan})
local gait = options.gait or (options.animate and assert(loadfile("assets/Monkey/Spider Worker/gait_poses.lua"))())
local part, pose = "",nil
local function point(x,y)
  if pose then return gait.project(pose,part,x,y) end
  return {x,y}
end

-- Filled polygons use pixel-centre scanlines; no smoothing or new colours.
local function poly(cel, points, colour)
  local projected = {}
  for i,p in ipairs(points) do projected[i] = point(p[1],p[2]) end
  points = projected
  for y = 0, 63 do
    local cuts = {}
    for i = 1, #points do
      local a, b = points[i], points[i % #points + 1]
      if (a[2] <= y + 0.5 and b[2] > y + 0.5) or
         (b[2] <= y + 0.5 and a[2] > y + 0.5) then
        cuts[#cuts+1] = a[1] + (y+0.5-a[2]) * (b[1]-a[1]) / (b[2]-a[2])
      end
    end
    table.sort(cuts)
    for i = 1, #cuts-1, 2 do
      local x1, x2 = math.ceil(cuts[i]-0.5), math.ceil(cuts[i+1]-0.5)-1
      if x1 <= x2 then axi.line(cel, x1, y, x2, y, colour) end
    end
  end
end
local function path(cel, points, colour, width)
  for i = 1, #points-1 do
    local a,b = point(points[i][1],points[i][2]), point(points[i+1][1],points[i+1][2])
    axi.line(cel, a[1], a[2], b[1], b[2], colour, { thickness=width or 1 })
  end
end
local function line(cel,x1,y1,x2,y2,colour)
  path(cel,{{x1,y1},{x2,y2}},colour)
end

local bg = axi.layer("Preview background")
axi.rect(axi.cel(bg,1), 0,0,64,64,"#819447",{fill=true})
bg.isVisible = false
local guides = axi.layer("Guides")
path(axi.cel(guides,1), {{32,44},{48,52},{32,60},{16,52},{32,44}}, tan)
guides.isVisible = false
if options.setupLayers then options.setupLayers(spr) end

local function draw(frame)
part = "tail"
local tail = axi.cel(axi.layer(options.tailLayer and options.tailLayer(pose) or "Tail"),frame)
-- Thick root, narrow raised shaft and an open inward curl.
poly(tail, {{29,38},{26,43},{21,40},{18,37},{16,34},{14,30},{12,23},{11,16},{10,12},{10,8},{12,5},{15,3},{20,3},{23,5},{25,8},{25,12},{23,15},{19,16},{16,14},{15,11},{17,9},{19,9},{18,11},{18,12},{20,13},{22,12},{22,9},{20,7},{16,6},{13,8},{13,12},{15,17},{16,24},{18,30},{19,33},{22,35},{25,37}}, ink)
-- Continuous structural stroke at the bend: overlapping segments prevent
-- transformed cels from opening a transparent pinhole at the hip.
path(tail, {{28,39},{24,37},{20,33},{18,28},{17,23}}, ink, 3)
path(tail, {{27,39},{22,37},{18,32},{16,26},{14,17},{12,12},{12,8},{15,5},{19,5},{22,7},{23,10},{22,13},{20,14}}, plum,2)
path(tail, {{13,7},{15,5},{19,5},{21,6}}, light)
path(tail, {{14,17},{16,25},{17,28}}, light)

part = "farLeg"
local far = axi.cel(axi.layer("Far limbs"),frame)
if not (options.drawLeg and options.drawLeg(far,pose,false)) then
poly(far, {{34,35},{38,38},{40,43},{37,48},{36,50},{40,51},{41,53},{37,54},{33,53},{32,50},{34,44},{31,40}}, ink)
poly(far, {{35,39},{37,41},{37,44},{35,48},{34,50},{33,49},{35,43}}, plum)
poly(far, {{35,51},{38,51},{39,52},{35,52}}, skin)
end
part = "farArm"
poly(far, {{41,25},{44,28},{44,35},{46,41},{46,47},{44,50},{41,49},{42,46},{43,46},{42,40},{40,34},{39,29}}, ink)
path(far, {{42,29},{42,34},{44,41},{44,45}}, plum)
path(far, {{44,47},{43,48}}, skin)

part = "body"
local body = axi.cel(axi.layer("Body"),frame)
if options.drawBody and pose.direction ~= "SE" then
  options.drawBody(body, pose)
else
poly(body, {{33,20},{39,20},{42,25},{40,31},{36,35},{35,40},{31,43},{26,42},{24,38},{25,31},{28,25}}, ink)
poly(body, {{32,22},{37,22},{36,26},{32,30},{29,35},{28,40},{26,38},{27,31},{29,26}}, plum)
poly(body, {{32,23},{34,23},{31,27},{29,31},{28,32},{29,27}}, light)
poly(body, {{34,31},{37,29},{36,33},{32,37},{31,38},{31,35}}, plum)
-- Open chest and a grouped tummy-fur highlight: light stays connected and
-- sparse, with the warmest shade reserved for the centre of the belly.
poly(body, {{30,27},{33,27},{35,31},{34,36},{32,39},{30,37},{29,32}}, plum)
poly(body, {{31,29},{33,29},{34,32},{33,36},{31,35},{30,32}}, light)
poly(body, {{32,31},{33,31},{33,34},{32,35}}, tan)
-- Small crown and forward/downward muzzle establish the elevated view.
poly(body, {{34,16},{37,13},{41,13},{44,15},{45,17},{48,19},{47,21},{46,22},{46,25},{43,28},{39,27},{37,25},{34,24},{33,20}}, ink)
poly(body, {{36,16},{38,14},{41,14},{43,16},{44,18},{40,17},{37,18},{35,20}}, plum)
path(body, {{38,14},{40,14},{42,15}}, light)
poly(body, {{39,22},{41,21},{46,22},{45,25},{43,27},{40,25}}, skin)
line(body,40,22,41,22,ink)
line(body,44,22,45,22,ink)
line(body,43,24,44,24,ink)
line(body,42,23,42,23,tan)
line(body,43,26,43,26,tan)
line(body,36,22,36,23,skin)
end

part = "nearLeg"
local near = axi.cel(axi.layer("Near limbs"),frame)
if not (options.drawLeg and options.drawLeg(near,pose,true)) then
poly(near, {{28,37},{33,38},{34,42},{32,47},{31,52},{33,54},{36,55},{36,57},{32,58},{28,57},{26,55},{27,51},{28,47},{27,42}}, ink)
poly(near, {{29,40},{31,40},{31,43},{29,48},{29,52},{28,54},{27,54},{29,46}}, plum)
path(near, {{29,41},{29,44},{28,47}}, light)
poly(near, {{29,54},{31,54},{34,56},{32,57},{29,56}}, skin)
line(near,29,55,30,55,light)
line(near,31,56,31,56,ink)
line(near,33,56,33,56,ink)
end
part = "nearArm"
local arm = options.armLayer and axi.cel(axi.layer(options.armLayer(pose)),frame) or near
-- Long near arm hangs clear of the thigh, ending in a compact hooked hand.
poly(arm, {{33,25},{37,25},{38,28},{36,34},{38,39},{39,45},{39,50},{37,53},{34,52},{34,50},{36,50},{36,46},{35,41},{32,36},{30,32},{31,28}}, ink)
poly(arm, {{33,27},{35,26},{35,30},{33,33},{35,38},{37,42},{37,47},{36,47},{35,41},{32,36},{31,32}}, plum)
path(arm, {{33,28},{32,31},{32,33}}, light)
path(arm, {{35,39},{36,42},{36,44}}, light)
path(arm, {{37,49},{37,51},{36,51}}, skin)
if options.drawPayload then options.drawPayload(frame, pose) end
end

if options.animate then
  axi.frames(#gait.frames-1)
  for f,p in ipairs(gait.frames) do
    pose = p
    draw(f)
    if options.finishFrame then options.finishFrame(f, p, spr) end
    axi.duration(f,p.ms)
  end
  for _,clip in ipairs(gait.clips) do axi.tag(clip.name,clip.first,clip.last,{direction="forward"}) end
  axi.save(options.output or "assets/Monkey/Spider Worker/spider_monkey_animations.ase")
else
  draw(1)
  spr.frames[1].duration = 0.25
  axi.save("assets/Monkey/Spider Worker/spider_monkey_idle.ase")
end
