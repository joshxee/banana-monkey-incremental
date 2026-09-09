-- sprite-axi run "assets/Monkey/Spider Worker/verify_animations.lua" -f "assets/Monkey/Spider Worker/spider_monkey_animations.ase"
-- Read-only acceptance checks on the rendered frames, not drawing internals.
local spr = app.activeSprite
assert(spr.width==64 and spr.height==64 and #spr.frames==28)
local allowed = {}
for _,hex in ipairs({"#3D3333","#593E47","#7A5859","#734C44","#BCAD9F"}) do
  allowed[Color{r=tonumber(hex:sub(2,3),16),g=tonumber(hex:sub(4,5),16),b=tonumber(hex:sub(6,7),16)}.rgbaPixel]=true
end
local frames = {}
for f=1,#spr.frames do
  local img = Image(64,64,ColorMode.RGB)
  img:drawSprite(spr,f)
  frames[f]=img
  local count=0
  for y=0,63 do for x=0,63 do
    local p=img:getPixel(x,y)
    if app.pixelColor.rgbaA(p)>0 then
      assert(allowed[p],"palette drift in frame "..f)
      assert(x>0 and x<63 and y>0 and y<63,"clipped frame "..f)
      count=count+1
    end
    if f>=2 and f<=4 and y>=54 then
      assert(p==frames[1]:getPixel(x,y),"idle foot/hand anchor drift")
    end
  end end
  assert(count>600 and count<1200,"unexpected silhouette area")
  -- The tail start must be a solid silhouette: every row in the connector
  -- band has at least one opaque pixel in the expected x range.
  if f>=1 then
    for y=28,40 do
      local connected=false
      for x=15,31 do if app.pixelColor.rgbaA(img:getPixel(x,y))>0 then connected=true break end end
      assert(connected,"tail connector hole at frame "..f.." row "..y)
    end
  end
  print("frame "..f..": "..count.." opaque pixels; palette and margins PASS")
end
print("all-fours idle contact pixels remain planted PASS")
local function diff(a,b)
  local n=0
  for y=0,63 do for x=0,63 do
    if frames[a]:getPixel(x,y)~=frames[b]:getPixel(x,y) then n=n+1 end
  end end
  return n
end
for _,clip in ipairs({{1,4},{11,22}}) do
  local min,max=4096,0
  for f=clip[1],clip[2]-1 do
    local n=diff(f,f+1)
    assert(n>0,"duplicate adjacent loop poses")
    min=math.min(min,n) max=math.max(max,n)
  end
  local seam=diff(clip[2],clip[1])
  assert(seam<=max*1.3,"loop seam exceeds internal frame changes")
  print("clip "..clip[1].."-"..clip[2]..": internal changes "..min.."-"..max..", loop seam "..seam.." PASS")
end
for _,pair in ipairs({{1,5},{10,11},{11,23},{28,1}}) do
  assert(diff(pair[1],pair[2])==0,"transition endpoint mismatch")
end
print("idle/rise/walk/settle boundary poses match exactly PASS")
local expected={{"idle",1,4,1100},{"rise",5,10,430},{"walk",11,22,720},{"settle",23,28,450}}
for _,clip in ipairs(expected) do
  local total=0 local found=false
  for f=clip[2],clip[3] do total=total+math.floor(spr.frames[f].duration*1000+0.5) end
  assert(total==clip[4],"clip duration mismatch")
  for _,tag in ipairs(spr.tags) do
    if tag.name==clip[1] then
      assert(tag.fromFrame.frameNumber==clip[2] and tag.toFrame.frameNumber==clip[3]) found=true
    end
  end
  assert(found,"missing tag")
end
print("tags and clip durations PASS")
