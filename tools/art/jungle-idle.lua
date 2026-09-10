-- sprite-axi run tools/art/jungle-idle.lua
-- Very slight, one-pixel leaf-tip idles for the five approved vegetation assets.
local out='assets/Jungle/'
local names={'jungle-broad','jungle-leaning','banana-fruiting','banana-harvested','jungle-fern'}
local W,H,N,MS=320,352,16,200
local palImage=Image{fromFile='docs/references/palette.png'}
local allowed={};for x=0,30 do allowed[palImage:getPixel(x,0)]=true end
local function clamp(v) return math.max(0,math.min(1,v)) end
local function round(v) return math.floor(v+.5) end
local function diff(a,b)
 local d=0;for y=0,a.height-1 do for x=0,a.width-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then d=d+1 end end end;return d
end
local clips={}
for _,name in ipairs(names) do
 local src=app.open(out..name..'.aseprite')
 assert(src.width==W and src.height==H and #src.frames==1,'Expected static vegetation master')
 local s=axi.new(W,H);s:deleteLayer(s.layers[1]);s:setPalette(src.palettes[1])
 axi.frames(N-1,{duration=MS});s.frames[1].duration=MS/1000
 local banana=name:match('^banana')~=nil
 local fern=name=='jungle-fern'
 local function moves(layerName)
  if banana then return layerName=='03 Back leaf paddles' or layerName=='04 Lateral torn paddles' or layerName=='05 Foreground drooping leaves' end
  if fern then return layerName=='02 Low fern fronds' end
  return layerName=='04 Broad unoutlined crown' or layerName=='04 Offset open crown'
 end
 for _,base in ipairs(src.layers) do
  local sourceCel=base:cel(1);local image=Image(sourceCel.image);local pos=sourceCel.position
  local layer=s:newLayer();layer.name=base.name;layer.isVisible=base.isVisible;layer.opacity=base.opacity
  for f=1,N do
   local img=image
   if moves(base.name) and f~=1 then
    img=Image(image.width,image.height,ColorMode.RGB)
    local t=2*math.pi*(f-1)/N
    local wave=math.sin(t)
    if name=='jungle-leaning' then wave=-math.sin(t)+.1*math.sin(2*t) end
    if fern then wave=.9*math.sin(t)-.1*math.sin(2*t) end
    for y=0,img.height-1 do for x=0,img.width-1 do
     local gx,gy=x+pos.x,y+pos.y
     local tip
     if banana then tip=clamp(math.sqrt((gx-160)^2+(gy-204)^2)/100)
     elseif fern then tip=clamp(math.sqrt((gx-160)^2+(gy-311)^2)/63)
     else tip=clamp(math.abs(gx-160)/115) end
     local dx=round(.95*wave*tip)
     assert(math.abs(dx)<=1,'Idle exceeds one native pixel')
     -- Backwards sampling preserves solid leaf shapes; there is no vertical bob.
     local sx=x-dx
     if sx>=0 and sx<image.width then img:drawPixel(x,y,image:getPixel(sx,y)) end
    end end
   end
   s:newCel(layer,f,img,pos)
  end
  -- Static source images are never mutated; verify their final copy too.
  if not moves(base.name) then assert(diff(image,layer:cel(N).image)==0,'Static structure moved') end
 end
 app.activeSprite=s;axi.tag('idle',1,N,{direction='forward'})
 local frames={}
 for f=1,N do
  local img=Image(W,H,ColorMode.RGB);img:drawSprite(s,f);frames[f]=img
  for y=0,H-1 do for x=0,W-1 do
   local p=img:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
   assert(a==0 or a==255,'Partial alpha')
   if a>0 then assert(allowed[p],'Palette drift');assert(x>0 and x<W-1 and y>0 and y<H-1,'Animation clipping') end
  end end
 end
 local still=Image(W,H,ColorMode.RGB);still:drawSprite(src,1)
 assert(diff(still,frames[1])==0,'Approved first frame changed')
 local maxStep,total,seam=0,0,0
 for f=1,N do
  local d=diff(frames[f],frames[f%N+1]);total=total+d
  if f==N then seam=d else maxStep=math.max(maxStep,d) end
 end
 assert(total>0,'Missing idle movement');assert(seam<=maxStep,'Wraparound jump')
 -- The source's anchor pixel and immediate stem/root center remain identical.
 for f=2,N do for y=306,319 do for x=154,166 do
  assert(frames[f]:getPixel(x,y)==frames[1]:getPixel(x,y),'Ground contact moved')
 end end end
 axi.save(out..name..'-idle.aseprite')
 local sheet=axi.new(W*4,H*4);sheet:deleteLayer(sheet.layers[1]);sheet:setPalette(src.palettes[1])
 local atlas=axi.cel('Idle frames',1)
 for f,img in ipairs(frames) do atlas.image:drawImage(img,Point(((f-1)%4)*W,math.floor((f-1)/4)*H)) end
 axi.save(out..name..'-idle-sheet.png');sheet:close()
 clips[#clips+1]={name=name,frames=frames}
 print(string.format('%s PASS: 16x200 ms; displacement <=1 px; native first frame, palette, alpha, margins, static structure, root contact; max adjacent %d px, seam %d px.',name,maxStep,seam))
 s:close();src:close()
end
-- Fruiting/harvested leaves must remain aligned at every matching frame.
for f=1,N do for y=0,H-1 do for x=0,W-1 do
 if clips[3].frames[f]:getPixel(x,y)~=clips[4].frames[f]:getPixel(x,y) then
  assert(x>=163 and x<=198 and y>=199 and y<=291,'Harvest states lose alignment')
 end
end end end
local json=assert(io.open(out..'jungle-idle.json','w'))
json:write('{\n  "frameWidth":320,"frameHeight":352,"columns":4,\n  "anchor":{"x":160,"y":316},"loop":true,"durationMs":3200,\n  "animations":[\n')
for i,c in ipairs(clips) do
 json:write(string.format('    {"name":"%s","tag":"idle","image":"%s-idle-sheet.png","frames":[\n',c.name,c.name))
 for f=1,N do json:write(string.format('      {"x":%d,"y":%d,"w":320,"h":352,"durationMs":200}%s\n',((f-1)%4)*W,math.floor((f-1)/4)*H,f<N and ',' or '')) end
 json:write('    ]}'..(i<#clips and ',' or '')..'\n')
end
json:write('  ]\n}\n');json:close()
-- Reuse the exact accepted scale sheet, including its unchanged worker references.
local source=app.open(out..'jungle-scale-preview.aseprite')
local preview=axi.new(source.width,source.height);preview:deleteLayer(preview.layers[1]);preview:setPalette(source.palettes[1])
axi.frames(N-1,{duration=MS});preview.frames[1].duration=MS/1000
for _,base in ipairs(source.layers) do
 local layer=preview:newLayer();layer.name=base.name;layer.opacity=base.opacity;layer.isVisible=base.isVisible
 local c=base:cel(1)
 for f=1,N do
  if base.name=='Vegetation at native resolution' then
   local img=Image(source.width,source.height,ColorMode.RGB)
   for i,clip in ipairs(clips) do img:drawImage(clip.frames[f],Point(((i-1)%3)*320,32+math.floor((i-1)/3)*384)) end
   preview:newCel(layer,f,img,Point(0,0))
  else preview:newCel(layer,f,c.image,c.position) end
 end
end
app.activeSprite=preview;axi.tag('idle',1,N,{direction='forward'})
local a=Image(source.width,source.height,ColorMode.RGB);a:drawSprite(source,1)
local b=Image(preview.width,preview.height,ColorMode.RGB);b:drawSprite(preview,1)
assert(diff(a,b)==0,'Scale-preview first frame differs from approved sheet')
axi.save(out..'jungle-idle-preview.aseprite');preview:saveCopyAs(out..'jungle-idle-preview.gif')
-- Compare opposing poses without changing native scale.
local motion=axi.new(source.width*2,source.height)
local mc=axi.cel('Frames 5 and 13',1)
for i,f in ipairs({5,13}) do local img=Image(source.width,source.height,ColorMode.RGB);img:drawSprite(preview,f);mc.image:drawImage(img,Point((i-1)*source.width,0)) end
axi.save(out..'jungle-idle-motion.png')
print('PASS: harvest states stay aligned throughout; exact original scale sheet at frame 1; five one-pixel idles exported.')
