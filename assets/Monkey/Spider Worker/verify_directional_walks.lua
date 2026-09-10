-- Read-only visual asset contracts, invoked with sprite-axi run ... -f master.
local root="assets/Monkey/Spider Worker/"
local s=app.activeSprite
assert(s.width==64 and s.height==64 and #s.frames==192,"master layout")
local paletteImage=Image{fromFile="docs/references/palette.png"}
local allowed={}
for x=0,paletteImage.width-1 do allowed[paletteImage:getPixel(x,0)]=true end
local sheets={Image{fromFile=root.."spider_monkey_walk_8dir.png"},Image{fromFile=root.."spider_monkey_carry_walk_8dir.png"}}
local f=assert(io.open(root.."spider_monkey_directional_walks.json","r"))
local meta=json.decode(f:read("*a"));f:close()
assert(meta.anchor.x==32 and meta.anchor.y==56 and #meta.clips==16)
local frames={}
local gold={}
for _,hex in ipairs({"FDD179","DE9F47","FEE1B8"}) do
  gold[Color{r=tonumber(hex:sub(1,2),16),g=tonumber(hex:sub(3,4),16),b=tonumber(hex:sub(5,6),16)}.rgbaPixel]=true
end
local minArea,maxArea,minGold=4096,0,4096
for i=1,192 do
  local img=Image(64,64,ColorMode.RGB);img:drawSprite(s,i);frames[i]=img
  local state=math.floor((i-1)/96)
  local row=math.floor(((i-1)%96)/12)
  local phase=(i-1)%12
  local area,fruit=0,0
  for y=0,63 do for x=0,63 do
    local p=img:getPixel(x,y)
    local alpha=app.pixelColor.rgbaA(p)
    assert(alpha==0 or alpha==255,"partial alpha: "..i)
    assert(p==sheets[state+1]:getPixel(phase*64+x,row*64+y),"source/export mismatch: "..i)
    if alpha>0 then
      assert(allowed[p],"palette drift: "..i)
      assert(x>0 and x<63 and y>0 and y<63,"clipped frame: "..i.." at "..x..","..y)
      area=area+1
      if gold[p] then fruit=fruit+1 end
    end
  end end
  assert(area>=400 and area<=1200,"silhouette size: "..i.." area "..area)
  if state==0 then assert(fruit==0,"fruit in empty-handed pose")
  else assert(fruit>=20,"occluded banana: "..i);minGold=math.min(minGold,fruit) end
  minArea=math.min(minArea,area);maxArea=math.max(maxArea,area)
end
for state=0,1 do
  for _,pair in ipairs({{2,8},{3,7},{4,6}}) do
    for phase=1,12 do
      local a=frames[state*96+(pair[1]-1)*12+phase]
      local b=frames[state*96+(pair[2]-1)*12+phase]
      for y=0,63 do for x=0,63 do
        assert(a:getPixel(x,y)==b:getPixel(63-x,y),"mirror mismatch")
      end end
    end
  end
end
local function diff(a,b)
  local n=0
  for y=0,63 do for x=0,63 do if a:getPixel(x,y)~=b:getPixel(x,y) then n=n+1 end end end
  return n
end
for i,clip in ipairs(meta.clips) do
  local first,last=clip.asepriteFirstFrame,clip.asepriteLastFrame
  assert(first==(i-1)*12+1 and last==i*12 and #clip.frames==12)
  local tag=s.tags[i]
  assert(tag.name==clip.name and tag.fromFrame.frameNumber==first and tag.toFrame.frameNumber==last,"tag mismatch")
  local ms,maxChange=0,0
  for n=first,last do
    ms=ms+math.floor(s.frames[n].duration*1000+0.5)
    assert(math.floor(s.frames[n].duration*1000+0.5)==meta.frameDurationMs[n-first+1])
    local rect=clip.frames[n-first+1]
    assert(rect.x==(n-first)*64 and rect.y==clip.row*64 and rect.w==64 and rect.h==64)
    if n<last then
      local delta=diff(frames[n],frames[n+1]);assert(delta>0,"duplicate pose")
      maxChange=math.max(maxChange,delta)
    end
  end
  local seam=diff(frames[last],frames[first])
  assert(ms==720 and seam>0 and seam<=maxChange*1.3,"loop seam: "..clip.name)
  print(clip.name..": 12 frames / "..ms.."ms; seam "..seam.." <= 1.3 * "..maxChange.." PASS")
end
-- Carry and empty hands retain exact tail/body/far limb pixels and leg timing.
for _,layer in ipairs(s.layers) do
  if layer.name=="Tail" or layer.name=="Tail foreground" or layer.name=="Body"
    or layer.name=="Far limbs" or layer.name=="Near limbs" then
    for i=1,96 do
      local a,b=Image(64,64,ColorMode.RGB),Image(64,64,ColorMode.RGB)
      local ca,cb=layer:cel(i),layer:cel(i+96)
      assert((ca==nil)==(cb==nil),"carry cel presence mismatch")
      if ca then a:drawImage(ca.image,ca.position);b:drawImage(cb.image,cb.position) end
      assert(diff(a,b)==0,"carry alignment drift: "..layer.name.." frame "..i)
    end
  end
end
-- The human review found detached down-facing feet. Every lower silhouette
-- pixel must connect to the torso, including skin highlights and toe pixels.
for state=0,1 do for phase=1,12 do
  local index=state*96+48+phase
  local img=frames[index]
  local queue,seen={{32,30}},{[30*64+32]=true}
  assert(app.pixelColor.rgbaA(img:getPixel(32,30))==255,"missing torso seed")
  local head=1
  while head<=#queue do
    local x,y=table.unpack(queue[head]);head=head+1
    for dy=-1,1 do for dx=-1,1 do
      local nx,ny=x+dx,y+dy
      local key=ny*64+nx
      if nx>=0 and nx<64 and ny>=0 and ny<64 and not seen[key]
        and app.pixelColor.rgbaA(img:getPixel(nx,ny))>0 then
        seen[key]=true;queue[#queue+1]={nx,ny}
      end
    end end
  end
  for y=43,63 do for x=0,63 do
    if app.pixelColor.rgbaA(img:getPixel(x,y))>0 then
      assert(seen[y*64+x],"detached lower-limb pixel: frame "..index.." at "..x..","..y)
    end
  end end
end end
local layers={}
for _,layer in ipairs(s.layers) do layers[layer.name]=layer end
assert(layers["Rear-view arm"].stackIndex<layers.Body.stackIndex,"rear arm must be behind body")
assert(layers["Tail foreground"].stackIndex>layers.Body.stackIndex,"rear tail must be in front of body")
local function layerImage(name,index)
  local img=Image(64,64,ColorMode.RGB)
  local c=layers[name]:cel(index)
  if c then img:drawImage(c.image,c.position) end
  return img
end
local armChecks,tailChecks=0,0
for state=0,1 do for _,row in ipairs({0,1,7}) do for phase=1,12 do
  local index=state*96+row*12+phase
  local body=layerImage("Body",index)
  local arm=layerImage("Rear-view arm",index)
  local tail=layerImage("Tail foreground",index)
  local upper=Image(64,64,ColorMode.RGB)
  for _,name in ipairs({"Near limbs","Near arm","Tail foreground","Banana","Banana grip"}) do
    upper:drawImage(layerImage(name,index))
  end
  assert(not layers.Tail:cel(index) and not layers["Near arm"]:cel(index),"rear parts in front-view slots")
  for y=0,63 do for x=0,63 do
    local bp,ap,tp=body:getPixel(x,y),arm:getPixel(x,y),tail:getPixel(x,y)
    if app.pixelColor.rgbaA(bp)>0 then
      if app.pixelColor.rgbaA(ap)>0 and app.pixelColor.rgbaA(upper:getPixel(x,y))==0 then
        assert(frames[index]:getPixel(x,y)==bp,"rear arm paints over torso")
        armChecks=armChecks+1
      end
      if app.pixelColor.rgbaA(tp)>0 then
        assert(frames[index]:getPixel(x,y)==tp,"rear tail hidden by torso")
        tailChecks=tailChecks+1
      end
    end
  end end
end end end
assert(armChecks>0 and tailChecks>0,"no rear overlap pixels checked")
print("PASS: all 24 south-facing poses have connected feet; "..armChecks.." arm/body and "..tailChecks.." tail/body overlap pixels.")
-- Approved SE remains exactly the same as the original cycle.
local legacy=app.open(root.."spider_monkey_animations.ase")
for phase=1,12 do
  local img=Image(64,64,ColorMode.RGB);img:drawSprite(legacy,10+phase)
  assert(diff(img,frames[36+phase])==0,"approved SE changed")
end
legacy:close()
print("PASS: 192 source/export matches; palette, binary alpha, margins, state alignment, exact legacy SE.")
print("Silhouette area "..minArea.."-"..maxArea.."; minimum visible banana pixels "..minGold..".")
