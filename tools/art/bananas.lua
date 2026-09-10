-- Run from repository root: sprite-axi run tools/art/bananas.lua
-- ASEPRITE_BIN must point to your installed Aseprite executable.
local out='assets/Banana/'
local W,H=48,48
local s=axi.new(W,H);s:deleteLayer(s.layers[1])
local paletteImage=Image{fromFile='docs/references/palette.png'}
local pal=Palette(31);local allowed={}
for x=0,30 do local p=paletteImage:getPixel(x,0);pal:setColor(x,Color(p));allowed[p]=true end
s:setPalette(pal)
local C={gold='#fdd179',shade='#de9f47',cream='#fee1b8',stem='#819447',stemShade='#546756',tip='#734c44',shadow='#546756'}
local cel
local function layer(name) cel=axi.cel(name,1) end
local function poly(points,color) axi.poly(cel,points,C[color],{fill=true}) end
local function line(x,y,xx,yy,color) axi.line(cel,x,y,xx,yy,C[color]) end
local function rect(x,y,w,h,color) axi.rect(cel,x,y,w,h,C[color],{fill=true}) end
-- Three authored, overlapping crescents: no enclosing contour or noise texture.
layer('02 Back banana')
poly({{12,18},{15,21},{19,22},{24,22},{28,19},{30,18},{32,20},{30,24},{26,27},{21,28},{17,26},{14,23}},'shade')
poly({{12,18},{15,20},{20,22},{25,21},{29,18},{30,20},{27,24},{22,26},{18,25},{15,22}},'gold')
line(15,21,19,23,'cream');line(19,23,23,23,'cream');line(12,17,12,18,'tip')
layer('03 Middle banana')
poly({{12,24},{15,26},{20,27},{25,25},{29,21},{31,20},{33,22},{31,28},{27,31},{22,33},{18,32},{14,28}},'shade')
poly({{12,24},{16,26},{21,27},{26,24},{30,20},{31,22},{29,27},{25,30},{21,31},{17,29},{14,27}},'gold')
line(16,27,19,28,'cream');line(19,28,23,28,'cream');line(23,28,26,26,'cream');line(11,23,12,24,'tip')
layer('04 Front banana')
poly({{18,29},{21,31},{25,30},{29,27},{31,22},{32,20},{34,21},{35,25},{33,30},{30,33},{26,35},{23,35},{20,33}},'shade')
poly({{18,29},{22,31},{26,30},{30,26},{32,21},{33,22},{33,27},{30,31},{26,33},{23,33},{20,31}},'gold')
line(22,31,25,31,'cream');line(26,30,29,27,'cream');line(18,28,18,29,'tip')
layer('05 Cut stalk')
poly({{29,20},{30,16},{30,14},{33,14},{32,17},{34,20},{32,22}},'stemShade')
poly({{30,19},{31,16},{31,14},{33,14},{32,17},{33,20},{32,21}},'stem')
line(31,14,33,14,'cream')
-- Insert the ground shadow below the fruit. Its 2:1 footprint is an optional layer.
layer('01 Ground shadow');cel.layer.stackIndex=1
for y=-3,3 do local dx=math.floor(7*math.sqrt(1-(y/3.5)^2));rect(24-dx,36+y,dx*2+1,1,'shadow') end
layer('06 Glint and action flecks')
local bases={}
for _,l in ipairs(s.layers) do bases[#bases+1]={layer=l,image=Image(l:cel(1).image),pos=l:cel(1).position} end
local clips={
 {name='still',durations={1000},loop=false},
 {name='spawn',durations={50,50,50,50,60,60,70,90,120},loop=false},
 {name='idle',durations={900,100,100,100,100,100,100,100,100,100,100,900},loop=true},
 {name='despawn',durations={60,50,50,50,50,60,70,100},loop=false}
}
local spawn={{0,0,0},{.4,.4,-3},{.7,.8,-4},{.95,1.08,-3},{1.08,.9,0},{1.03,.96,0},{1,1,0},{1,1,0},{1,1,0}}
local despawn={{1,1,0},{1.06,.92,0},{.94,1.05,-3},{.8,.9,-6},{.58,.66,-10},{.3,.4,-13},{0,0,0},{0,0,0}}
local function transform(base,sx,sy,dy,shadow)
 local img=Image(W,H,ColorMode.RGB)
 if sx==0 then return img end
 if shadow then sy=sx;dy=0 end
 for y=0,H-1 do for x=0,W-1 do
  local xx=math.floor((x-24)/sx+24+.5)-base.pos.x
  local yy=math.floor((y-36-dy)/sy+36+.5)-base.pos.y
  if xx>=0 and yy>=0 and xx<base.image.width and yy<base.image.height then img:drawPixel(x,y,base.image:getPixel(xx,yy)) end
 end end
 return img
end
local nextFrame=1
for _,clip in ipairs(clips) do
 clip.first=nextFrame;clip.frames={};clip.total=0
 for i,ms in ipairs(clip.durations) do
  local f=nextFrame;if f>1 then s:newEmptyFrame() end
  s.frames[f].duration=ms/1000;clip.total=clip.total+ms
  local pose={1,1,0}
  if clip.name=='spawn' then pose=spawn[i] elseif clip.name=='despawn' then pose=despawn[i] end
  if f>1 then for _,base in ipairs(bases) do
   local img=transform(base,pose[1],pose[2],pose[3],base.layer.name=='01 Ground shadow')
   s:newCel(base.layer,f,img,Point(0,0))
  end end
  cel=axi.cel('06 Glint and action flecks',f)
  -- A quiet two-pixel highlight travels along the existing front ridge. No bob.
  if clip.name=='idle' and i>=3 and i<=8 then
   local p=({{23,32},{24,32},{26,31},{27,30},{28,29},{29,28}})[i-2]
   rect(p[1],p[2],1,1,'cream');rect(p[1]+1,p[2],1,1,'cream')
  elseif clip.name=='spawn' and i>=4 and i<=6 then
   local d=i-4;rect(9-d,27-d,2,1,'gold');rect(37+d,24-d,1,2,'gold')
  elseif clip.name=='despawn' and i>=5 and i<=7 then
   local d=i-5;rect(19-d,17-d,1,1,'gold');rect(30+d,13-d,1,1,'cream')
  end
  local flat=Image(W,H,ColorMode.RGB);flat:drawSprite(s,f)
  clip.frames[#clip.frames+1]=flat;nextFrame=nextFrame+1
 end
 clip.last=nextFrame-1
end
-- Create tags after all frames: Aseprite extends a tag when appending at its end.
for _,clip in ipairs(clips) do axi.tag(clip.name,clip.first,clip.last,{direction='forward'}) end
axi.save(out..'banana-bunch.aseprite')
local function equal(a,b)
 for y=0,H-1 do for x=0,W-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end
 return true
end
local still=clips[1].frames[1]
assert(equal(still,clips[2].frames[9]),'Spawn must settle into still')
assert(equal(still,clips[3].frames[1]) and equal(still,clips[3].frames[12]),'Idle loop boundary')
assert(equal(still,clips[4].frames[1]),'Despawn must start at still')
assert(equal(clips[2].frames[1],clips[4].frames[8]),'Empty endpoints')
local idleChanges=0
for _,img in ipairs(clips[3].frames) do for y=0,H-1 do for x=0,W-1 do
 local a,b=still:getPixel(x,y),img:getPixel(x,y)
 assert(app.pixelColor.rgbaA(a)==app.pixelColor.rgbaA(b),'Idle silhouette moved')
 if a~=b then
  idleChanges=idleChanges+1
  assert(x>=22 and x<=30 and y>=27 and y<=32,'Idle changed outside front highlight')
 end
end end end
assert(idleChanges>0,'Idle has no visible change')
local disk=app.open(out..'banana-bunch.aseprite')
for _,clip in ipairs(clips) do
 local tag
 for _,t in ipairs(disk.tags) do if t.name==clip.name then tag=t end end
 assert(tag and tag.fromFrame.frameNumber==clip.first and tag.toFrame.frameNumber==clip.last,'Saved tag mismatch')
 for i,img in ipairs(clip.frames) do
  local check=Image(W,H,ColorMode.RGB);check:drawSprite(disk,clip.first+i-1)
  assert(equal(img,check),'Saved master differs')
  assert(math.abs(disk.frames[clip.first+i-1].duration*1000-clip.durations[i])<.01,'Saved timing mismatch')
 end
end
disk:close();app.activeSprite=s
local colors={};local frameCount=0
local json=assert(io.open(out..'banana-bunch.json','w'))
json:write('{\n  "source":"banana-bunch.aseprite","frameWidth":48,"frameHeight":48,\n  "anchor":{"x":24,"y":36},"sampling":"nearest","frameIndexBase":0,\n  "animations":{\n')
for ci,clip in ipairs(clips) do
 local sheet=Image(W*#clip.frames,H,ColorMode.RGB)
 for i,img in ipairs(clip.frames) do
  frameCount=frameCount+1
  for y=0,H-1 do for x=0,W-1 do
   local p=img:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
   assert(a==0 or a==255,'Partial alpha')
   if a>0 then assert(allowed[p],'Palette drift');assert(x>0 and x<W-1 and y>0 and y<H-1,'Clipping');colors[p]=true end
  end end
  sheet:drawImage(img,Point((i-1)*W,0))
 end
 local filename='banana-bunch-'..clip.name..(clip.name=='still' and '.png' or '-sheet.png')
 sheet:saveAs(out..filename)
 local exported=Image{fromFile=out..filename}
 for y=0,H-1 do for x=0,sheet.width-1 do assert(exported:getPixel(x,y)==sheet:getPixel(x,y),'PNG differs from source') end end
 json:write(string.format('    "%s":{"image":"%s","loop":%s,"durationMs":%d,"asepriteFrames":[%d,%d],"frames":[',clip.name,filename,tostring(clip.loop),clip.total,clip.first,clip.last))
 for i,ms in ipairs(clip.durations) do json:write(string.format('%s{"x":%d,"y":0,"w":48,"h":48,"durationMs":%d}',i>1 and ',' or '',(i-1)*48,ms)) end
 json:write(']}'..(ci<#clips and ',' or '')..'\n')
end
json:write('  }\n}\n');json:close()
-- Review-only enlarged image and full lifecycle GIF with the exact native worker.
local worker=Image{fromFile='docs/references/spider-worker.png'}
local function reviewFrame(img)
 local p=Image(144,88,ColorMode.RGB);p:clear(app.pixelColor.rgba(164,197,175,255))
 p:drawImage(worker,Point(10,12));p:drawImage(img,Point(78,38))
 return p
end
local p=axi.new(576,352);p:setPalette(pal)
local function enlarged(img)
 local dest=Image(576,352,ColorMode.RGB)
 for y=0,351 do for x=0,575 do dest:drawPixel(x,y,img:getPixel(math.floor(x/4),math.floor(y/4))) end end
 return dest
end
local review=reviewFrame(still);review:saveAs(out..'banana-bunch-native-preview.png')
p.layers[1].name='Review only - worker and backdrop'
s:close();app.activeSprite=p
local n=0
for _,ci in ipairs({2,3,4}) do for i,img in ipairs(clips[ci].frames) do
 n=n+1;if n>1 then p:newEmptyFrame() end
 p.frames[n].duration=clips[ci].durations[i]/1000
 if ci==4 and i==8 then p.frames[n].duration=.7 end
 p:newCel(p.layers[1],n,enlarged(reviewFrame(img)),Point(0,0))
end end
p:saveCopyAs(out..'banana-bunch-preview.gif')
axi.save(out..'banana-bunch-preview.aseprite')
enlarged(review):saveAs(out..'banana-bunch-preview.png')
local colorCount=0;for _ in pairs(colors) do colorCount=colorCount+1 end
print(string.format('PASS: %d frames, %d palette colors; binary alpha, margins, exact PNG/source agreement; spawn/still/idle/despawn seams align; 48x48 cells, anchor (24,36).',frameCount,colorCount))
print('PASS: saved master pixels, tags and timings; idle silhouette fixed, '..idleChanges..' highlight pixel changes across the loop.')
for _,clip in ipairs(clips) do print(clip.name..': '..#clip.frames..' frames, '..clip.total..' ms, loop='..tostring(clip.loop)) end
