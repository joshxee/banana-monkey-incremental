-- Art-directed joints based on the user's spider-monkey video (especially
-- 0.0–1.3s and 16.0–17.8s): upright weight transfers, wide loose arms, quick feet.
-- Idle is quadrupedal; rise/settle are one-shot clips with exact boundary poses.
local M = {frames={},clips={}}
local function clamp(v) return math.max(0,math.min(1,v)) end
local function mix(a,b,t) return a+(b-a)*t end
local function smooth(v) v=clamp(v) return v*v*(3-2*v) end
local function copy(p)
  local q={} for k,v in pairs(p) do q[k]=v end return q
end
local rest={stand=0,resting=1,tailAway=1,roll=0,bob=0,tail=0,nearStep=0,farStep=0,nearLift=0,farLift=0,nearReach=0,farReach=0,nearUp=0,farUp=0,breath=0}
local function clip(name,poses)
  local first=#M.frames+1
  for _,p in ipairs(poses) do M.frames[#M.frames+1]=p end
  M.clips[#M.clips+1]={name=name,first=first,last=#M.frames,loop=name=="idle" or name=="walk"}
end
local idle={}
for i,v in ipairs({{0,0,300},{0.7,0.5,250},{0.7,1,300},{0,0.5,250}}) do
  idle[i]=copy(rest) idle[i].breath=v[1] idle[i].tail=v[2] idle[i].ms=v[3]
end
clip("idle",idle)

-- A 720ms cycle: short recoveries separated by a tiny weight-catching pause.
-- Stance soles translate up-left in the local canvas, opposite world travel.
-- Wider footfall than the old shuffle: the monkey catches weight under the
-- body, then reaches a long leg out before the next catch.
local steps={6,5,2,0,-2,-5,-6,-5,-2,0,2,5}
local lifts={0,0,0,0,0,1,0,2,5,6,5,2}
local rolls={-0.7,-1.2,-1.5,-1.1,-0.4,0.3,0.7,1.2,1.5,1.1,0.4,-0.3}
local bobs={0,0.6,0.4,-0.3,-0.8,-0.5,0,0.6,0.4,-0.3,-0.8,-0.5}
local reaches={0,-1,-2,-2,-1,0,1,2,2,1,0,0}
local ups={0,0,1,2,3,2,1,0,0,0,1,1}
local times={70,60,50,70,60,50,70,60,50,70,60,50}
local walk={}
for i=1,12 do
  local other=(i+5)%12+1
  local p=copy(rest)
  p.stand=1 p.roll=rolls[i] p.bob=bobs[i] p.tail=-rolls[(i+9)%12+1]
  p.nearStep=steps[i] p.farStep=steps[other]
  p.nearLift=lifts[i] p.farLift=lifts[other]
  p.nearReach=reaches[i] p.farReach=-reaches[other]
  p.nearUp=ups[i] p.farUp=ups[other]
  p.ms=times[i] walk[i]=p
end
local rise={}
local amounts={0,0.12,0.34,0.60,0.84,1}
local riseTimes={80,80,80,70,60,60}
for i,t in ipairs(amounts) do
  local p={}
  for k,v in pairs(rest) do p[k]=mix(v,walk[1][k],smooth(t)) end
  p.stand=t p.ms=riseTimes[i] rise[i]=p
end
clip("rise",rise)
clip("walk",walk)
local settle={}
for i=1,6 do settle[i]=copy(rise[7-i]) settle[i].ms=({60,60,70,80,80,100})[i] end
clip("settle",settle)

local function body(p,x,y)
  -- At rest the spine is only gently pitched: the rump sits over the bent
  -- legs instead of folding the whole monkey into a deep hunch.
  local restAngle = p.resting==1 and 0.22 or 0.55
  local angle=mix(restAngle,0.04,p.stand)+p.roll*0.035
  local dx,dy=x-29,y-40
  local weight=clamp((44-y)/17)
  local pelvisLift=p.resting==1 and -1 or 0
  return {29+dx*math.cos(angle)-dy*math.sin(angle)+p.roll*0.5,
          40+dx*math.sin(angle)+dy*math.cos(angle)+p.bob+pelvisLift-p.breath*weight}
end
-- Retarget original pixel contours through shoulder/elbow/wrist or hip/knee/
-- ankle joints. End shapes translate rigidly, preserving fingers and soles.
local function limb(x,y,source,target)
  local a,b=1,2
  if y>=source[3][2] then return {x+target[3][1]-source[3][1],y+target[3][2]-source[3][2]} end
  if y>source[2][2] then a,b=2,3 end
  local t=(y-source[a][2])/(source[b][2]-source[a][2])
  local oldX=mix(source[a][1],source[b][1],t)
  return {mix(target[a][1],target[b][1],t)+x-oldX,mix(target[a][2],target[b][2],t)}
end
function M.project(p,part,x,y)
  local out
  if part=="body" then out=body(p,x,y)
  elseif part=="tail" then
    local root=body(p,26,40)
    local t=clamp((40-y)/37)
    -- Follow the pelvis at the root, then let the middle and curl lag behind
    -- with an eased counter-sway. This removes the visible hinge at the rump.
    local base=1-clamp((y-36)/10)
    local sway=p.tail * (0.16*t + 0.84*t*t)
    local lift=p.tail * 0.12*t*(1-t)
    out={x+(root[1]-26)*base+sway,
         y+(root[2]-40)*base-lift}
    -- The tail curl opens out into space, away from the monkey's chest.
    if p.tailAway==1 and y<30 then out[1]=34-out[1] end
  elseif part=="nearLeg" or part=="farLeg" then
    local near=part=="nearLeg"
    local src=near and {{30,39},{30,46},{29,54}} or {{35,38},{37,44},{35,51}}
    local hip=body(p,src[1][1],src[1][2])
    local step,lift=near and p.nearStep or p.farStep,near and p.nearLift or p.farLift
    local ankle={src[3][1]+step,src[3][2]+step*0.5-lift}
    local knee={mix(hip[1],ankle[1],0.55)-1-lift*0.55,
                mix(hip[2],ankle[2],0.53)-lift*0.1}
    out=limb(x,y,src,{hip,knee,ankle})
  else
    local near=part=="nearArm"
    local src=near and {{34,27},{34,36},{37,50}} or {{41,28},{43,37},{44,48}}
    local shoulder=body(p,src[1][1],src[1][2])
    -- Hands stay planted through the first part of the rise; near releases
    -- before far. During the walk both float well above their ground planes.
    local release=smooth((p.stand-(near and 0.12 or 0.35))/(near and 0.88 or 0.65))
    -- Splayed palms sit below the shoulders during the all-fours rest. The
    -- hands open out to either side rather than hanging beneath the muzzle.
    local planted=near and (p.resting==1 and {39,56} or {46,56})
                         or (p.resting==1 and {48,53} or {54,52})
    local reach=near and p.nearReach or p.farReach
    local up=near and p.nearUp or p.farUp
    local airborne=near and {23+p.roll*1.4+reach,45-up+p.bob*0.3}
                       or {52+p.roll*0.7+reach,43-up+p.bob*0.3}
    local wrist={mix(planted[1],airborne[1],release),mix(planted[2],airborne[2],release)}
    local elbow={mix(shoulder[1],wrist[1],0.50)+(near and -1.5 or 1.5)*release,
                 mix(shoulder[2],wrist[2],0.48)-release}
    out=limb(x,y,src,{shoulder,elbow,wrist})
  end
  return {math.floor(out[1]+0.5),math.floor(out[2]+0.5)}
end
return M
