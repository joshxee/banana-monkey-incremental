-- Add a restrained, seamless idle to the approved static building.
-- sprite-axi run tools/art/town-center-idle.lua -f assets/TownCenter/town-center.aseprite
local source=app.activeSprite
assert(source.width==672 and source.height==704 and #source.frames==1,'Expected approved static master')
local out='assets/TownCenter/'
local count,duration=16,150
local motionStrength=.5 -- Half the original breeze; round only after scaling.
local base={}
for _,layer in ipairs(source.layers) do
 local c=layer:cel(1)
 base[#base+1]={name=layer.name,image=Image(c.image),position=Point(c.position.x,c.position.y),opacity=layer.opacity,visible=layer.isVisible}
end
local s=axi.new(source.width,source.height)
s:deleteLayer(s.layers[1]);s:setPalette(source.palettes[1])
axi.frames(count-1,{duration=duration})
s.frames[1].duration=duration/1000
local function round(n) return math.floor(n+.5) end
local function clamp(n) return math.max(0,math.min(1,n)) end
local animated={['07 Flat crown with broken leaf clusters']=true,['09 Civic cloth and gathered belongings']=true}
for _,b in ipairs(base) do
 local layer=s:newLayer();layer.name=b.name;layer.opacity=b.opacity;layer.isVisible=b.visible
 for f=1,count do
  local img=b.image
  if animated[b.name] and f~=1 then
   img=Image(b.image.width,b.image.height,ColorMode.RGB)
   local t=2*math.pi*(f-1)/count
   for y=0,img.height-1 do for x=0,img.width-1 do
    local gx,gy=x+b.position.x,y+b.position.y
    local dx,dy=0,0
    if b.name=='07 Flat crown with broken leaf clusters' then
     -- Bend outlying boughs more than the trunk junction. Sample backwards so
     -- adjacent pixels remain solid rather than opening holes during a warp.
     local tip=clamp(math.abs(gx-330)/220)
     local height=.35+.65*clamp((240-gy)/180)
     local sway=math.sin(t)*(.55+2.4*tip)+math.sin(2*t)*.4*(gx<330 and -1 or 1)
     if gy>240 then sway=sway+math.sin(t)*clamp((gy-240)/50)*1.2 end
     dx=round(sway*motionStrength)
     dy=round(math.sin(t)*tip*height*.8*motionStrength)
    elseif gx>=409 and gx<=452 and gy>=342 and gy<=416 then
     -- Cloth stays fixed along its diagonal top seam. Jars and rolled mats
     -- belong to the same source layer and are intentionally not moved.
     local drop=clamp((gy-(567-gx*.5))/55)
     dx=round((3.2*math.sin(t)+.6*math.sin(2*t))*drop*drop*motionStrength)
    end
    local sx,sy=x-dx,y-dy
    if sx>=0 and sx<img.width and sy>=0 and sy<img.height then img:drawPixel(x,y,b.image:getPixel(sx,sy)) end
   end end
  end
  s:newCel(layer,f,img,b.position)
 end
end
app.activeSprite=s
axi.tag('idle',1,count,{direction='forward'})
local allowed={}
local palImage=Image{fromFile='docs/references/palette.png'}
for x=0,30 do allowed[palImage:getPixel(x,0)]=true end
local frames={}
for f=1,count do
 local img=Image(s.width,s.height,ColorMode.RGB);img:drawSprite(s,f);frames[f]=img
 for y=0,s.height-1 do for x=0,s.width-1 do
  local p=img:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
  assert(a==0 or a==255,'Partial alpha in idle')
  if a>0 then
   assert(allowed[p],'Palette drift in idle')
   assert(x>0 and x<s.width-1 and y>0 and y<s.height-1,'Clipped idle')
  end
 end end
end
local function diff(a,b)
 local changed=0
 for y=0,s.height-1 do for x=0,s.width-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then changed=changed+1 end end end
 return changed
end
local first=Image(source.width,source.height,ColorMode.RGB);first:drawSprite(source,1)
assert(diff(first,frames[1])==0,'Frame one changed approved artwork')
local maximum,total,seam=0,0,0
for f=1,count do
 local d=diff(frames[f],frames[f%count+1]);total=total+d
 if f==count then seam=d else maximum=math.max(maximum,d) end
end
assert(total>0,'Idle has no motion')
assert(seam<=maximum,'Large wraparound jump')
-- Every non-animated layer, and every pixel of the collection area, is fixed.
for _,layer in ipairs(s.layers) do if not animated[layer.name] then
 local a=layer:cel(1).image
 for f=2,count do local b=layer:cel(f).image
  assert(a.width==b.width and a.height==b.height)
  for it in a:pixels() do assert(it()==b:getPixel(it.x,it.y),'Static layer moved') end
 end
end end
axi.save(out..'town-center-idle.aseprite')
-- Packed, fixed-size transparent atlas: four columns by four rows.
local sheet=axi.new(s.width*4,s.height*4)
sheet:deleteLayer(sheet.layers[1]);sheet:setPalette(source.palettes[1])
local atlas=axi.cel('Idle frames',1)
for f,img in ipairs(frames) do atlas.image:drawImage(img,Point(((f-1)%4)*s.width,math.floor((f-1)/4)*s.height)) end
axi.save(out..'town-center-idle-sheet.png');sheet:close()
local metadata=assert(io.open(out..'town-center-idle.json','w'))
metadata:write('{\n  "image": "town-center-idle-sheet.png",\n  "frameWidth": 672, "frameHeight": 704, "columns": 4,\n  "animation": "idle", "loop": true, "durationMs": 2400,\n  "frames": [\n')
for f=1,count do metadata:write(string.format('    {"x": %d, "y": %d, "w": 672, "h": 704, "durationMs": 150}%s\n',((f-1)%4)*s.width,math.floor((f-1)/4)*s.height,f<count and ',' or '')) end
metadata:write('  ]\n}\n');metadata:close()
-- Native-resolution, centered preview with unchanged scale-reference workers.
local preview=axi.new(704,672)
preview:deleteLayer(preview.layers[1]);preview:setPalette(source.palettes[1])
axi.frames(count-1,{duration=duration});preview.frames[1].duration=duration/1000
local background=preview:newLayer();background.name='Muted ground'
local building=preview:newLayer();building.name='Building idle'
local monkeys=preview:newLayer();monkeys.name='Reference workers - unchanged'
local bg=Image(704,672,ColorMode.RGB);bg:clear(Color{r=164,g=197,b=175,a=255})
local worker=Image{fromFile='docs/references/spider-worker.png'}
local refs=Image(704,672,ColorMode.RGB)
local positions={{124,485},{446,552},{271,375}}
for _,p in ipairs(positions) do refs:drawImage(worker,Point(p[1],p[2])) end
for f=1,count do
 preview:newCel(background,f,bg,Point(0,0))
 preview:newCel(building,f,frames[f],Point(32,14))
 preview:newCel(monkeys,f,refs,Point(0,0))
end
app.activeSprite=preview;axi.tag('idle',1,count,{direction='forward'})
axi.save(out..'town-center-idle-preview.aseprite')
preview:saveCopyAs(out..'town-center-idle-preview.gif')
-- Three extremes for a still inspection of the native pixel deformation.
local motion=axi.new(704*3,672)
local c=axi.cel('Frames 1, 5, 13',1)
for i,f in ipairs({1,5,13}) do local img=Image(704,672,ColorMode.RGB);img:drawSprite(preview,f);c.image:drawImage(img,Point((i-1)*704,0)) end
axi.save(out..'town-center-idle-motion.png')
print(string.format('PASS: %d frames at %d ms, 2400 ms loop. Approved frame 1 unchanged. Static layers unchanged. Exact reference palette and binary alpha. Adjacent maximum %d changed px, seam %d changed px.',count,duration,maximum,seam))
