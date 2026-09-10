-- Run from the repository root with sprite-axi. Original contours are shared
-- with create_idle.lua; the approved SE walk is reproduced pixel for pixel.
local root = "assets/Monkey/Spider Worker/"
local base = assert(loadfile(root.."gait_poses.lua"))()
local gait = {frames={}, clips={}}
local directions = {"N","NE","E","SE","S","SW","W","NW"}
local canonical = {SW="SE",W="E",NW="NE"}
local C = {ink="#3D3333",plum="#593E47",light="#7A5859",skin="#734C44",tan="#BCAD9F",
  gold="#FDD179",ochre="#DE9F47",cream="#FEE1B8"}
local function round(n) return math.floor(n+0.5) end
local function mix(a,b,t) return a+(b-a)*t end
local function clamp(n) return math.max(0,math.min(1,n)) end
for state=0,1 do
  for _,d in ipairs(directions) do
    local first=#gait.frames+1
    for phase=1,12 do
      local p={}
      for k,v in pairs(base.frames[10+phase]) do p[k]=v end
      p.direction=canonical[d] or d
      p.mirror=canonical[d]~=nil
      p.carry=state==1
      p.phase=phase
      gait.frames[#gait.frames+1]=p
    end
    gait.clips[#gait.clips+1]={name=(state==0 and "walk_" or "carry_walk_")..d,
      first=first,last=#gait.frames}
  end
end

-- Each view has an authored shoulder/hip separation and screen-space travel
-- axis. Diagonal ground motion is 2:1; height remains vertical in every view.
local views={
 S={axis={0,0.65}, hips={{27,39},{35,39}}, feet={{25,53},{38,53}},
    shoulders={{25,26},{38,26}}, hands={{18,44},{46,44}}, hold={41,34}},
 E={axis={1,0}, hips={{30,39},{34,37}}, feet={{28,55},{33,53}},
    shoulders={{33,26},{39,26}}, hands={{25,46},{47,41}}, hold={46,33}},
 NE={axis={1,-0.5}, hips={{30,39},{35,37}}, feet={{29,55},{36,52}},
     shoulders={{30,25},{40,24}}, hands={{22,45},{49,40}}, hold={47,32}},
 N={axis={0,-0.65}, hips={{36,39},{27,39}}, feet={{39,55},{25,55}},
    shoulders={{38,26},{25,26}}, hands={{46,44},{18,44}}, hold={44,32}},
 SE={hold={45,35}},
}
local function bodyShift(p) return round(p.roll*0.5),round(p.bob-1) end
local function hold(p)
  local dx,dy=bodyShift(p)
  return {views[p.direction].hold[1]+dx,views[p.direction].hold[2]+dy}
end
local function retarget(x,y,source,target)
  if y>=source[3][2] then
    return {x+target[3][1]-source[3][1],y+target[3][2]-source[3][2]}
  end
  local a,b=1,2
  if y>source[2][2] then a,b=2,3 end
  local t=(y-source[a][2])/(source[b][2]-source[a][2])
  return {mix(target[a][1],target[b][1],t)+x-mix(source[a][1],source[b][1],t),
    mix(target[a][2],target[b][2],t)}
end
local function legJoints(p,near)
  local v=views[p.direction]
  local i=near and 1 or 2
  local dx,dy=bodyShift(p)
  local step=near and p.nearStep or p.farStep
  local lift=near and p.nearLift or p.farLift
  local hip={v.hips[i][1]+dx,v.hips[i][2]+dy}
  local ankle={v.feet[i][1]+step*v.axis[1],v.feet[i][2]+step*v.axis[2]-lift}
  local knee={mix(hip[1],ankle[1],0.55)-v.axis[1]*(1+lift*0.5),
    mix(hip[2],ankle[2],0.53)-lift*0.1}
  return hip,knee,ankle
end
function gait.project(p,part,x,y)
  local out
  local v=views[p.direction]
  local dx,dy=bodyShift(p)
  if p.direction=="SE" and not (p.carry and part=="nearArm") then
    out=base.project(p,part,x,y)
  elseif part=="tail" then
    out=base.project(p,part,x,y)
    -- Front/rear views leave the curl beside the crown, never over the face.
    -- Side views retain the long raised shaft of the approved worker.
    local root=clamp((y-26)/14)
    local offset=p.direction=="N" and 6 or p.direction=="S" and 1 or 0
    out[1]=out[1]+offset*root
  elseif part=="nearLeg" or part=="farLeg" then
    local near=part=="nearLeg"
    local src=near and {{30,39},{30,46},{29,54}} or {{35,38},{37,44},{35,51}}
    local hip,knee,ankle=legJoints(p,near)
    out=retarget(x,y,src,{hip,knee,ankle})
    if y>=src[3][2] then
      local fx,fy=x-src[3][1],y-src[3][2]
      if p.direction=="N" or p.direction=="S" then
        out={ankle[1]+fy-fx*0.5-1,ankle[2]+fx*v.axis[2]+fy*0.3}
      elseif p.direction=="NE" then
        out={ankle[1]+fx,ankle[2]+fy-fx}
      end
    end
  elseif part=="nearArm" or part=="farArm" then
    local near=part=="nearArm"
    local i=near and 1 or 2
    local src=near and {{34,27},{34,36},{37,50}} or {{41,28},{43,37},{44,48}}
    local shoulder
    if p.direction=="SE" then shoulder=base.project(p,"body",34,27)
    else shoulder={v.shoulders[i][1]+dx,v.shoulders[i][2]+dy} end
    local wrist,elbow
    if p.carry and near then
      wrist=hold(p)
      elbow={mix(shoulder[1],wrist[1],0.42)-2,math.max(shoulder[2],wrist[2])+6}
    else
      local reach=near and p.nearReach or p.farReach
      local up=near and p.nearUp or p.farUp
      wrist={v.hands[i][1]+dx+reach,v.hands[i][2]+dy-up}
      elbow={mix(shoulder[1],wrist[1],0.5)+(near and -1 or 1),mix(shoulder[2],wrist[2],0.5)-1}
    end
    out=retarget(x,y,src,{shoulder,elbow,wrist})
  else out=base.project(p,part,x,y) end
  out={round(out[1]),round(out[2])}
  return out
end

-- Rasterize without antialiasing. These are authored front, profile and back
-- contours, not a rotated flat sprite. Left views are reflected after drawing
-- so line and polygon fill conventions cannot introduce a one-pixel mismatch.
local function drawing(cel,p,shift)
  local dx,dy=0,0
  if shift then dx,dy=bodyShift(p) end
  local function point(x,y)
    x,y=round(x+dx),round(y+dy)
    return {x,y}
  end
  local function poly(points,color)
    local pts={}
    for _,q in ipairs(points) do pts[#pts+1]=point(q[1],q[2]) end
    -- Match the scanline fill convention of the original worker contours.
    for y=0,63 do
      local cuts={}
      for i,a in ipairs(pts) do
        local b=pts[i%#pts+1]
        if (a[2]<=y+0.5 and b[2]>y+0.5) or (b[2]<=y+0.5 and a[2]>y+0.5) then
          cuts[#cuts+1]=a[1]+(y+0.5-a[2])*(b[1]-a[1])/(b[2]-a[2])
        end
      end
      table.sort(cuts)
      for i=1,#cuts-1,2 do
        local a,b=math.ceil(cuts[i]-0.5),math.ceil(cuts[i+1]-0.5)-1
        if a<=b then axi.line(cel,a,y,b,y,C[color]) end
      end
    end
  end
  local function path(points,color,width)
    for i=1,#points-1 do
      local a,b=point(table.unpack(points[i])),point(table.unpack(points[i+1]))
      axi.line(cel,a[1],a[2],b[1],b[2],C[color],{thickness=width or 1})
    end
  end
  return poly,path
end
local function drawLeg(cel,p,near)
  if p.direction~="S" then return false end
  local hip,knee,ankle=legJoints(p,near)
  local hx,hy=table.unpack(hip)
  local kx,ky=table.unpack(knee)
  local ax,ay=table.unpack(ankle)
  local poly,path=drawing(cel,p,false)
  -- A continuous shin meets a compact forward-pointing foot. Rotating only
  -- the old profile's toe vertices folded its outline across the ankle and
  -- left detached skin/highlight pixels. These contours share an ankle area.
  poly({{hx-3,hy-2},{hx+3,hy-1},{kx+2,ky},{ax+1,ay+1},
    {ax-2,ay+1},{kx-2,ky+1},{hx-3,hy+2}},"ink")
  poly({{ax-2,ay-1},{ax+1,ay-1},{ax+2,ay+1},{ax+2,ay+4},
    {ax,ay+5},{ax-3,ay+4},{ax-3,ay+1}},"ink")
  path({{hx-1,hy+1},{kx-1,ky},{ax-1,ay}},"plum",2)
  if near then path({{hx-2,hy+1},{kx-2,ky}},"light") end
  poly({{ax-1,ay+1},{ax+1,ay+1},{ax+1,ay+3},{ax,ay+4},{ax-2,ay+3}},"skin")
  path({{ax-1,ay+2},{ax,ay+2}},"light")
  return true
end
local function rearView(p) return p.direction=="N" or p.direction=="NE" end
local function setupLayers()
  -- Per-view cels keep editable parts and a stable global layer stack. Empty
  -- rear/front slots prevent a change of facing from changing other frames.
  for _,name in ipairs({"Tail","Far limbs","Rear-view arm","Body","Near limbs",
    "Near arm","Tail foreground","Banana","Banana grip"}) do axi.layer(name) end
end
local function drawBody(cel,p)
  local poly,path=drawing(cel,p,true)
  if p.direction=="S" then
    poly({{27,23},{36,23},{40,27},{38,33},{36,37},{37,42},{32,44},{26,42},{25,36},{23,29}},"ink")
    poly({{27,24},{31,24},{29,31},{28,37},{29,41},{26,40},{25,32}},"plum")
    poly({{29,27},{35,27},{36,32},{34,39},{31,41},{29,36},{28,31}},"plum")
    poly({{30,29},{34,29},{35,32},{33,37},{31,36},{29,32}},"light")
    poly({{31,31},{33,31},{33,34},{32,35}},"tan")
    poly({{24,18},{26,15},{29,13},{35,13},{38,15},{40,19},{39,25},{36,28},{28,28},{24,24}},"ink")
    poly({{26,18},{28,15},{33,14},{36,15},{38,18},{34,18},{31,17},{28,19},{26,22}},"plum")
    path({{29,14},{33,14},{35,15}},"light")
    poly({{27,22},{30,21},{32,22},{34,21},{38,22},{37,25},{34,27},{30,27},{27,25}},"skin")
    path({{28,22},{29,22}},"ink");path({{35,22},{36,22}},"ink")
    path({{31,24},{33,24}},"ink");path({{31,23},{32,23}},"tan")
    path({{32,26},{33,26}},"tan")
    path({{24,22},{24,23}},"skin");path({{39,22},{39,23}},"skin")
  elseif p.direction=="E" then
    poly({{32,21},{38,22},{40,27},{37,33},{35,40},{32,43},{26,41},{26,35},{28,29}},"ink")
    poly({{32,23},{35,23},{33,29},{30,34},{29,40},{27,39},{28,32}},"plum")
    path({{32,24},{30,28},{29,31}},"light")
    poly({{35,28},{38,27},{36,33},{33,38},{32,36},{34,31}},"plum")
    path({{36,29},{35,33},{34,34}},"light")
    poly({{33,17},{36,14},{40,13},{43,15},{44,18},{49,20},{48,22},{46,23},{45,27},{41,28},{37,25},{34,24}},"ink")
    poly({{35,17},{37,15},{40,14},{43,16},{43,18},{40,17},{37,19},{35,21}},"plum")
    path({{38,14},{40,14},{42,15}},"light")
    poly({{42,21},{46,21},{48,22},{45,24},{45,26},{42,27},{40,24}},"skin")
    path({{44,21},{45,21}},"ink");path({{46,23},{47,23}},"ink")
    path({{42,23},{42,23}},"tan");path({{43,26},{43,26}},"tan")
    path({{37,22},{37,23}},"skin")
  else
    local front=p.direction=="NE"
    local shift=front and 3 or 0
    poly({{27+shift,23},{35+shift,22},{40,26},{38,32},{36,37},{37,42},{32,44},{26,42},{25,36},{24,29}},"ink")
    poly({{28,25},{33,24},{36,26},{34,31},{31,36},{30,41},{27,40},{27,34},{26,29}},"plum")
    poly({{29,25},{32,25},{32,27},{29,31},{28,32},{28,28}},"light")
    poly({{34,33},{37,30},{36,37},{34,40},{32,40}},"plum")
    if front then
      poly({{33,17},{36,14},{40,13},{43,15},{44,18},{46,20},{45,24},{42,27},{36,26},{33,23},{32,20}},"ink")
      poly({{34,17},{37,15},{40,14},{43,16},{43,20},{40,23},{37,24},{34,22}},"plum")
      path({{37,14},{40,14},{42,15}},"light")
      path({{44,21},{44,23}},"skin")
      path({{35,22},{35,23}},"skin")
    else
      poly({{24,19},{26,16},{29,13},{35,13},{38,16},{40,20},{39,24},{36,27},{28,27},{24,24}},"ink")
      poly({{26,19},{28,16},{31,14},{35,15},{38,18},{37,22},{34,24},{29,24},{26,22}},"plum")
      path({{29,15},{32,14},{35,15}},"light")
      path({{24,22},{24,23}},"skin");path({{39,22},{39,23}},"skin")
    end
  end
end
local function drawPayload(frame,p)
  if not p.carry then return end
  local h=hold(p)
  local cel=axi.cel(axi.layer("Banana"),frame)
  local poly,path=drawing(cel,p,false)
  local x,y=h[1],h[2]
  -- One curved banana, carried by the stem at the chest. Its crescent is
  -- offset from the body so the load remains readable in the rear views.
  poly({{x,y-3},{x+2,y-4},{x+4,y-2},{x+5,y+2},{x+4,y+6},{x+1,y+9},
    {x-3,y+10},{x-6,y+8},{x-2,y+7},{x+1,y+4},{x+1,y+1}},"ochre")
  poly({{x+2,y-2},{x+4,y+1},{x+3,y+5},{x,y+8},{x-3,y+9},{x-5,y+8},
    {x-1,y+6},{x+2,y+3}},"gold")
  path({{x+3,y+1},{x+3,y+3},{x+1,y+6},{x-2,y+8}},"cream")
  path({{x,y-4},{x+1,y-3}},"skin")
  -- Fingers are drawn last, wrapping the stem instead of disappearing behind
  -- the fruit. They share the wrist's exact rounded translation and phase.
  local grip=axi.cel(axi.layer("Banana grip"),frame)
  local gp,gl=drawing(grip,p,false)
  gp({{x-2,y-1},{x+1,y-1},{x+2,y+1},{x+1,y+3},{x-1,y+3},{x-2,y+1}},"ink")
  gl({{x-1,y},{x+1,y},{x+1,y+1}},"skin")
  gl({{x-1,y+2},{x,y+2}},"light")
end
local function finishFrame(frame,p,sprite)
  if not p.mirror then return end
  for _,layer in ipairs(sprite.layers) do
    local c=layer:cel(frame)
    if c and layer.isVisible then
      local img=Image(64,64,ColorMode.RGB)
      for y=0,c.image.height-1 do for x=0,c.image.width-1 do
        local gx,gy=x+c.position.x,y+c.position.y
        if gx>=0 and gx<64 and gy>=0 and gy<64 then
          img:drawPixel(63-gx,gy,c.image:getPixel(x,y))
        end
      end end
      c.image=img;c.position=Point(0,0)
    end
  end
end
assert(loadfile(root.."create_idle.lua"))({animate=true,gait=gait,drawBody=drawBody,
  drawLeg=drawLeg,setupLayers=setupLayers,
  tailLayer=function(p) return rearView(p) and "Tail foreground" or "Tail" end,
  armLayer=function(p) return rearView(p) and "Rear-view arm" or "Near arm" end,
  drawPayload=drawPayload,finishFrame=finishFrame,output=root.."spider_monkey_directional_walks.aseprite",
  palette={C.ink,C.plum,C.light,C.skin,C.tan,C.ochre,C.gold,C.cream}})
