-- sprite-axi run tools/art/baboon-chef-animation.lua
-- Restrained idle and tongs-lift loops derived from the editable v2 poses.
local out='assets/Monkey/Baboon Chef/v2/'
local source=app.open(out..'baboon-chef.aseprite')
local sourcePalette=source.palettes[1]
local palette,allowed=Palette(#sourcePalette),{}
for x=0,#sourcePalette-1 do local color=sourcePalette:getColor(x);allowed[color.rgbaPixel]=true;palette:setColor(x,color) end
local poses={{name='left',sourceFrame=2,mirror=false,rear=false},{name='right',sourceFrame=5,mirror=true,rear=false},
 {name='back-left',sourceFrame=3,mirror=false,rear=true},{name='back-right',sourceFrame=6,mirror=true,rear=true}}
local bases={};local layerNames={}
for _,l in ipairs(source.layers) do layerNames[#layerNames+1]=l.name end
local function flatten(s,f) local im=Image(64,64,ColorMode.RGB);im:drawSprite(s,f);return im end
for _,p in ipairs(poses) do
 p.parts={};bases[p.name]=flatten(source,p.sourceFrame)
 for _,l in ipairs(source.layers) do
  local c=l:cel(p.sourceFrame);local im=Image(64,64,ColorMode.RGB)
  if c then im:drawImage(c.image,c.position) end;p.parts[l.name]=im
 end
end
local spr=axi.new(64,64);spr:setPalette(palette);spr:deleteLayer(spr.layers[1])
source:close()
local modes={{name='idle',ms=180},{name='cook',ms=120}}
local lifts={0,0,0,1,1,2,2,2,1,1,1,0,0,0,0,0}
local tail={0,0,0,0,1,1,1,1,1,1,0,0,0,0,0,0}
local clips,frames={},{};local frame=0
local function same(a,b)
 for y=0,a.height-1 do for x=0,a.width-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end;return true
end
local function valid(im)
 for y=0,im.height-1 do for x=0,im.width-1 do local pixel=im:getPixel(x,y);local a=app.pixelColor.rgbaA(pixel)
  assert(a==0 or a==255,'Partial alpha')
  if a>0 then assert(allowed[pixel],'Palette drift');assert(x>0 and y>0 and x<im.width-1 and y<im.height-1,'Clipping') end
 end end
end
local atlas=Image(1024,512,ColorMode.RGB)
for _,mode in ipairs(modes) do for _,pose in ipairs(poses) do
 local clip={name=mode.name..'-'..pose.name,mode=mode.name,angle=pose.name,row=#clips,count=16,frameMs=mode.ms,
  durationMs=16*mode.ms,loop=true,asepriteFirstFrame=frame+1}
 clips[#clips+1]=clip
 for phase=1,16 do
  frame=frame+1;if frame>1 then spr:newEmptyFrame() end;spr.frames[frame].duration=mode.ms/1000
  local lift=mode.name=='cook' and lifts[phase] or 0
  for _,name in ipairs(layerNames) do
   local base=pose.parts[name];local im=Image(64,64,ColorMode.RGB)
   for y=0,63 do for x=0,63 do
    local pixel=base:getPixel(x,y)
    if app.pixelColor.rgbaA(pixel)>0 then
     local dy=0;local canonicalX=pose.mirror and 63-x or x
     if name=='01 Tail from rump' and y>=46 then dy=tail[phase] end
     if name=='09 Near arm and fingers' then dy=-math.floor(lift*math.max(0,math.min(1,(28-canonicalX)/10))+.5) end
     if name=='11 Iron tongs' then dy=-lift end
     im:drawPixel(x,y+dy,pixel)
    end
   end end
   if name=='07 Bare face and jaw' and not pose.rear and (phase==10 or phase==11) then
    for eyeY=0,63 do for eyeX=0,63 do
     if im:getPixel(eyeX,eyeY)==app.pixelColor.rgba(189,147,87,255) then
      im:drawPixel(eyeX,eyeY,app.pixelColor.rgba(80,71,71,255))
     end
    end end
   end
   spr:newCel(axi.layer(name),frame,im,Point(0,0))
  end
  local im=flatten(spr,frame);valid(im);frames[frame]=im
  atlas:drawImage(im,Point((phase-1)*64,clip.row*64))
  -- Every leg pixel remains exactly stationary across the whole loop.
  for _,name in ipairs({'02 Far leg','05 Near haunch and leg'}) do
   local c=axi.layer(name):cel(frame);assert(same(c.image,pose.parts[name]),'Foot drift')
  end
 end
 clip.asepriteLastFrame=frame
 assert(same(frames[clip.asepriteFirstFrame],bases[pose.name]),'First pose differs')
 assert(same(frames[clip.asepriteFirstFrame],frames[clip.asepriteLastFrame]),'Loop seam differs')
 assert(not same(frames[clip.asepriteFirstFrame],frames[clip.asepriteFirstFrame+5]),'Motion missing')
 local t=spr:newTag(clip.asepriteFirstFrame,clip.asepriteLastFrame);t.name=clip.name
end end
axi.save(out..'baboon-animations.aseprite');atlas:saveAs(out..'baboon-animations.png')
assert(same(atlas,Image{fromFile=out..'baboon-animations.png'}),'Atlas round trip')
local saved=app.open(out..'baboon-animations.aseprite');assert(#saved.frames==128 and #saved.tags==8)
for f=1,128 do assert(same(frames[f],flatten(saved,f)),'Saved frame differs');assert(math.floor(saved.frames[f].duration*1000+.5)==(f<=64 and 180 or 120),'Timing differs') end;saved:close()
local meta={source='baboon-animations.aseprite',sheet='baboon-animations.png',cell={width=64,height=64},atlasRowIndexBase=0,atlasColumnIndexBase=0,asepriteFrameIndexBase=1,
 clips=clips,anchors={left={x=28,y=59},right={x=35,y=59}},sampling='nearest',
 description='Idle: blink and one-pixel tail-tip movement. Cook: same idle details and a two-pixel tongs lift, arm bending around a fixed shoulder. Feet stay fixed. No walk or banana-flip animation.'}
local f=assert(io.open(out..'baboon-animations.json','w'));f:write(json.encode(meta));f:close()
-- A looping four-angle cooking GIF for a quick review without a browser.
local preview=Sprite(768,240,ColorMode.RGB);preview:setPalette(palette)
for phase=1,16 do
 if phase>1 then preview:newEmptyFrame() end;preview.frames[phase].duration=.12
 local im=Image(256,80,ColorMode.RGB);im:clear(app.pixelColor.rgba(164,197,175,255))
 for index=1,4 do im:drawImage(frames[clips[index+4].asepriteFirstFrame+phase-1],Point((index-1)*64,8)) end
 local big=Image(768,240,ColorMode.RGB)
 for y=0,239 do for x=0,767 do big:drawPixel(x,y,im:getPixel(math.floor(x/3),math.floor(y/3))) end end
 preview:newCel(preview.layers[1],phase,big,Point(0,0))
end
preview:saveCopyAs(out..'four-angle-cook.gif');preview:close()
local report='PASS: 128 frames, 8 loops, source character palette, binary alpha, clear margins, fixed leg layers, static first poses, exact loop endpoints, saved master/atlas pixels and frame durations.'
f=assert(io.open(out..'animation-validation.txt','w'));f:write(report..'\n');f:close();print(report)
