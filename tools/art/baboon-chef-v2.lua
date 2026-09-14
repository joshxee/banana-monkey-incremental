-- From repository root: sprite-axi run tools/art/baboon-chef-v2.lua
-- A new construction from the user's anatomy photographs. No v1 pixels used.
local out='assets/Monkey/Baboon Chef/v2/'
app.fs.makeAllDirectories(out)
local paletteImage=Image{fromFile='docs/references/palette.png'}
local palette,allowed=Palette(41),{}
for x=0,30 do local p=paletteImage:getPixel(x,0);palette:setColor(x,Color(p));allowed[p]=true end
-- User-approved palette flexibility: a close warm-grey ramp replaces the
-- alternating orange/grey bands. Extra colors are local to this character.
local C={deep='#403b39',shade='#60584f',brown='#756b5c',fur='#867b68',warm='#928671',
 light='#a0947f',face='#615453',faceLight='#756461',faceDark='#504747',eye='#bd9357',cream='#d5d6db',white='#f1f6f0',
 cloth='#96a9c1',iron='#303843',steel='#6c81a1'}
local extra={'deep','shade','brown','fur','warm','light','face','faceLight','faceDark','eye'}
for i,name in ipairs(extra) do local c=Color{r=tonumber(C[name]:sub(2,3),16),g=tonumber(C[name]:sub(4,5),16),b=tonumber(C[name]:sub(6,7),16),a=255};palette:setColor(30+i,c);allowed[c.rgbaPixel]=true end
local spr=axi.new(64,64);spr:setPalette(palette);spr:deleteLayer(spr.layers[1])
local cel,frame
local function layer(n) cel=axi.cel(n,frame) end
local function poly(p,c) axi.poly(cel,p,C[c],{fill=true}) end
local function line(p,c,w) for i=1,#p-1 do axi.line(cel,p[i][1],p[i][2],p[i+1][1],p[i+1][2],C[c],{thickness=w or 1}) end end
local function rect(x,y,w,h,c) axi.rect(cel,x,y,w,h,C[c],{fill=true}) end
local function draw(f,cook,rear)
 frame=f
 layer('01 Tail from rump')
 poly({{38,34},{42,36},{45,41},{47,46},{48,51},{51,55},{56,56},{59,55},{59,58},{54,59},{49,57},{45,53},{44,48},{42,43},{38,39}},'brown')
 poly({{40,37},{43,39},{45,44},{46,50},{49,55},{54,57},{58,57},{55,58},{50,57},{46,54},{44,49},{42,43}},'shade')
 line({{41,38},{44,42},{46,49},{48,53},{52,56},{56,57}},'fur')
 layer('02 Far leg')
 poly({{23,35},{29,37},{29,42},{25,46},{24,51},{26,54},{24,56},{18,56},{16,54},{18,52},{20,51},{20,46},{19,42}},'shade')
 poly({{24,38},{27,39},{26,43},{23,47},{23,52},{24,54},{20,54},{22,51},{21,46},{21,42}},'brown')
 line({{18,54},{22,55}},'face');rect(17,55,3,1,'deep')
 layer('03 Torso and shoulder')
 poly({{22,15},{29,15},{34,18},{37,23},{39,29},{40,35},{38,41},{33,43},{27,40},{24,35},{20,33},{17,29},{16,24},{18,19}},'brown')
 poly({{23,16},{29,16},{33,19},{35,23},{36,29},{35,34},{31,37},{26,35},{22,32},{19,29},{18,24},{20,20}},'fur')
 poly({{23,17},{28,17},{31,19},{32,22},{28,21},{25,22},{23,26},{20,26},{19,23},{21,19}},'warm')
 -- Quiet, connected shoulder mass rather than concentric highlight bands.
 poly({{34,29},{38,31},{38,37},{35,40},{31,39},{28,35},{31,35}},'brown')
 if rear then
  poly({{22,18},{28,16},{33,19},{36,24},{37,30},{35,35},{31,37},{26,33},{22,28},{20,23}},'fur')
  poly({{23,18},{28,17},{31,19},{32,22},{29,22},{27,24},{24,24},{21,22}},'warm')
  line({{34,26},{35,30},{34,33}},'brown')
 end
 layer('04 Far arm')
 poly({{19,24},{22,25},{22,30},{20,34},{17,39},{16,43},{17,45},{16,47},{14,46},{13,43},{14,38},{17,33},{18,28}},'brown')
 line({{19,27},{19,32},{17,36},{15,40}},'fur',2)
 poly({{14,41},{16,41},{15,44},{17,45},{16,47},{14,46},{13,43}},'faceDark')
 layer('05 Near haunch and leg')
 poly({{33,34},{38,35},{40,39},{38,44},{34,48},{33,51},{35,55},{34,58},{30,60},{24,60},{22,58},{24,56},{29,55},{27,51},{26,47},{28,42},{30,39}},'brown')
 poly({{33,36},{37,37},{38,40},{35,44},{30,48},{30,51},{32,55},{30,57},{28,54},{27,49},{28,45},{31,41}},'fur')
 poly({{33,36},{36,38},{36,41},{32,44},{30,44},{31,40}},'warm')
 poly({{29,55},{33,56},{33,58},{29,59},{24,59},{23,58},{25,57}},'faceDark')
 line({{25,57},{29,57}},'face');rect(23,59,3,1,'deep');rect(28,59,2,1,'deep')
 layer('06 Skull and cheek')
 -- The eye sits UNDER a low brow. The face descends from it; it is not a beak.
 poly({{14,11},{16,8},{20,6},{25,6},{28,8},{30,12},{30,17},{28,21},{24,23},{19,22},{15,19},{13,15}},'fur')
 poly({{15,11},{17,8},{21,7},{25,7},{27,9},{23,9},{20,10},{17,12}},'warm')
 line({{18,9},{21,8},{24,8}},'light')
 poly({{24,12},{28,13},{29,17},{27,21},{23,22},{21,20},{22,16}},'brown')
 poly({{26,14},{28,15},{28,18},{25,21},{23,20},{23,17}},'fur')
 layer('07 Bare face and jaw')
 if rear then
  -- Head turns away, with just the far muzzle projecting beyond the brow.
  poly({{14,12},{17,12},{17,15},{14,18},{13,20},{15,22},{18,21},{20,17},{19,13}},'faceDark')
  line({{15,16},{14,19},{15,20}},'face');rect(13,20,1,1,'deep')
  poly({{17,10},{22,8},{27,10},{29,14},{28,19},{24,22},{20,20},{17,17},{16,13}},'fur')
  poly({{19,10},{23,9},{26,11},{27,14},{25,16},{22,15},{20,17},{18,15}},'warm')
  poly({{25,15},{27,15},{27,18},{25,19},{24,17}},'brown')
 else
  -- Two brow planes and a broad nose end show the face turned toward us.
  -- The long sloping bridge still reaches well below the cheek.
  poly({{15,12},{19,12},{23,13},{25,16},{24,20},{25,23},{23,26},{19,27},{15,26},{12,24},{12,21},{14,18}},'faceDark')
  poly({{16,14},{19,14},{20,17},{18,20},{16,23},{13,23},{13,21},{15,18}},'faceLight')
  poly({{20,16},{23,15},{24,17},{23,20},{24,23},{22,25},{18,26},{15,25},{14,23},{18,22}},'face')
  poly({{14,21},{18,21},{20,23},{18,24},{14,24},{12,23}},'faceLight')
  line({{14,25},{18,26},{22,25}},'faceDark')
  rect(12,22,2,1,'deep');rect(17,23,1,1,'deep')
  line({{14,13},{17,12},{19,13}},'faceDark');line({{21,14},{23,14}},'faceDark')
  rect(16,14,1,1,'eye');rect(17,14,1,1,'deep');rect(22,15,1,1,'eye')
  -- Small ear high behind the eye; no round cartoon side ears.
  poly({{28,11},{29,12},{29,14},{27,15},{26,13}},'brown')
  rect(27,12,1,2,'faceDark')
 end
 layer('08 Apron and ties')
 if cook then
  if rear then
   line({{22,24},{28,30},{34,24}},'cream')
   line({{25,34},{35,35}},'cream')
   poly({{30,34},{28,37},{31,35},{33,38},{33,35}},'cloth')
  else
   -- Bib stays below the throat, leaving the muzzle and cheek wholly exposed.
   line({{21,25},{22,28}},'cream');line({{28,25},{27,29}},'cream')
   poly({{21,28},{27,29},{28,34},{29,38},{25,42},{20,39},{20,34}},'cloth')
   poly({{22,29},{26,30},{26,34},{27,38},{24,40},{21,38},{21,34}},'cream')
   line({{28,34},{35,35}},'cream')
  end
 end
 layer('09 Near arm and fingers')
 if cook then
  if rear then
   poly({{25,23},{29,23},{32,26},{33,30},{31,33},{27,34},{23,32},{19,29},{16,28},{16,26},{20,27},{25,29},{27,29},{27,27},{24,26}},'brown')
   poly({{26,24},{29,24},{31,27},{31,30},{28,32},{24,31},{21,29},{25,30},{28,30},{28,27}},'fur')
   poly({{16,26},{19,27},{18,29},{15,29},{14,27}},'faceDark')
  else
   poly({{28,23},{32,24},{35,27},{35,31},{32,35},{28,37},{24,37},{20,36},{17,36},{15,34},{16,32},{20,33},{25,34},{28,33},{29,31},{28,28},{26,27}},'brown')
   poly({{29,24},{32,26},{33,28},{33,31},{30,34},{27,35},{23,35},{21,34},{25,35},{29,34},{30,31},{30,28},{28,26}},'fur')
   poly({{16,32},{19,33},{19,35},{17,36},{15,35},{14,33}},'faceDark')
   line({{15,33},{17,33}},'face')
  end
 else
  poly({{25,22},{29,22},{32,24},{34,28},{34,31},{30,35},{28,39},{26,43},{26,47},{25,49},{23,48},{22,44},{23,39},{26,34},{27,29},{24,26}},'brown')
  poly({{26,23},{29,23},{31,25},{32,28},{32,31},{28,35},{26,39},{25,43},{23,42},{24,38},{27,33},{28,29},{27,26},{25,25}},'fur')
  line({{28,26},{29,29}},'warm')
  poly({{23,43},{25,44},{25,47},{26,48},{25,49},{23,48},{22,45}},'shade')
  line({{23,44},{23,46}},'face');rect(24,48,2,1,'deep')
 end
 layer('10 Small chef cap')
 if cook then
  poly({{18,7},{17,5},{18,3},{20,3},{21,2},{24,2},{25,4},{27,4},{27,6},{25,8},{21,8}},'cream')
  poly({{18,4},{20,4},{22,3},{24,3},{24,5},{26,5},{25,6},{20,6}},'white')
  line({{19,7},{24,7}},'cloth')
 end
 layer('11 Iron tongs')
 if cook then
  if rear then
   line({{16,28},{11,24},{8,23}},'iron');line({{16,28},{10,26},{7,25}},'steel')
   rect(8,22,1,2,'cream');rect(7,25,1,1,'cream')
  else
   line({{16,34},{11,34},{7,36}},'iron');line({{16,34},{11,36},{8,38}},'steel')
   rect(7,36,1,1,'cream');rect(8,38,1,1,'cream')
  end
 end
end
draw(1,false,false)
spr:newEmptyFrame();draw(2,true,false)
spr:newEmptyFrame();draw(3,true,true)
-- Mirror complete layers, including the tail. Left and right remain anatomical.
for f=1,3 do
 spr:newEmptyFrame()
 for _,l in ipairs(spr.layers) do local c=l:cel(f);if c then
  local im=Image(64,64,ColorMode.RGB)
  for y=0,c.image.height-1 do for x=0,c.image.width-1 do im:drawPixel(63-c.position.x-x,c.position.y+y,c.image:getPixel(x,y)) end end
  spr:newCel(l,f+3,im,Point(0,0))
 end end
end
local names={'anatomy-left','chef-left','chef-back-left','anatomy-right','chef-right','chef-back-right'}
local images={}
local function same(a,b) for y=0,a.height-1 do for x=0,a.width-1 do assert(a:getPixel(x,y)==b:getPixel(x,y),'Image mismatch') end end end
local function flatten(s,f) local im=Image(s.width,s.height,ColorMode.RGB);im:drawSprite(s,f);return im end
local count=0
local function export(im,path)
 for y=0,im.height-1 do for x=0,im.width-1 do
  local p=im:getPixel(x,y);local a=app.pixelColor.rgbaA(p);assert(a==0 or a==255,'Partial alpha')
  if a>0 then assert(allowed[p],'Palette drift');assert(x>0 and y>0 and x<im.width-1 and y<im.height-1,'Clipping') end
 end end
 im:saveAs(path);same(im,Image{fromFile=path});count=count+1
end
local sheet=Image(384,64,ColorMode.RGB)
for f,name in ipairs(names) do
 local t=spr:newTag(f,f);t.name=name;images[name]=flatten(spr,f)
 export(images[name],out..name..'.png');sheet:drawImage(images[name],Point((f-1)*64,0))
end
export(sheet,out..'baboon-sheet.png');axi.save(out..'baboon-chef.aseprite')
local disk=app.open(out..'baboon-chef.aseprite')
assert(#disk.frames==6 and #disk.layers==11)
for f,name in ipairs(names) do same(images[name],flatten(disk,f)) end;disk:close()
local function enlarged(im,k)
 local dst=Image(im.width*k,im.height*k,ColorMode.RGB)
 for y=0,dst.height-1 do for x=0,dst.width-1 do dst:drawPixel(x,y,im:getPixel(math.floor(x/k),math.floor(y/k))) end end
 return dst
end
local function background(w,h) local im=Image(w,h,ColorMode.RGB);im:clear(app.pixelColor.rgba(164,197,175,255));return im end
local study=background(232,80)
study:drawImage(Image{fromFile='docs/references/spider-worker.png'},Point(8,10))
study:drawImage(images['anatomy-left'],Point(84,8));study:drawImage(images['chef-left'],Point(160,8))
study:saveAs(out..'anatomy-and-chef-native.png');enlarged(study,4):saveAs(out..'anatomy-and-chef-4x.png')
local anatomy=background(72,72);anatomy:drawImage(images['anatomy-left'],Point(4,4));enlarged(anatomy,6):saveAs(out..'anatomy-6x.png')
-- Each back-row chef faces inward. The front chef is a rear three-quarter view.
-- The existing grill is only a context prop; this recipe replaces the baboons.
local grill=Image{fromFile='assets/BananaGrill/banana-grill.png'}
local slots={{pose='chef-right',x=33,y=51,anchorX=35,anchorY=59,depth='behind'},
 {pose='chef-left',x=132,y=51,anchorX=28,anchorY=59,depth='behind'},
 {pose='chef-back-right',x=70,y=89,anchorX=35,anchorY=59,depth='in-front'}}
local station=axi.new(224,176);station:setPalette(palette);station:deleteLayer(station.layers[1])
local states={}
for n=0,3 do
 local f=n+1;if f>1 then station:newEmptyFrame() end
 for i=1,2 do if i<=n then local p=slots[i];station:newCel(axi.layer('0'..i..' Chef '..i),f,images[p.pose],Point(p.x,p.y)) end end
 station:newCel(axi.layer('03 Grill'),f,grill,Point(48,37))
 if n==3 then local p=slots[3];station:newCel(axi.layer('04 Chef 3'),f,images[p.pose],Point(p.x,p.y)) end
 local tag=station:newTag(f,f);tag.name='chefs-'..n;states[n]=flatten(station,f)
 export(states[n],out..'grill-'..n..'-chefs.png')
end
axi.save(out..'grill-station.aseprite')
disk=app.open(out..'grill-station.aseprite');for n=0,3 do same(states[n],flatten(disk,n+1)) end;disk:close()
local scene=background(224,176);scene:drawImage(states[3]);enlarged(scene,3):saveAs(out..'grill-preview-3x.png')
local full=background(496,392)
local positions={{20,8},{252,8},{20,192},{252,192}}
for _,p in ipairs(positions) do full:drawImage(states[3],Point(p[1],p[2])) end
full:saveAs(out..'twelve-chefs.png')
for hired=0,15 do
 local visible=math.min(hired,12);local total=0
 for g=0,3 do local n=math.min(3,math.max(0,visible-g*3));total=total+n;assert(n>=0 and n<=3) end
 assert(total==visible)
end
local metadata={version=2,cell={width=64,height=64},sheet='baboon-sheet.png',frames=names,frameIndexBase=0,sampling='nearest',
 anchors={left={x=28,y=59},right={x=35,y=59}},source='baboon-chef.aseprite',layers=11,
 station={source='grill-station.aseprite',width=224,height=176,anchor={x=112,y=137},grillOffset={x=48,y=37},slots=slots},
 capacity={maxVisibleChefs=12,chefsPerGrill=3,maxVisibleGrills=4,policy='fill stable slots in grill order; visual capacity only'},
 palettePolicy='Canonical prop and clothing colors plus ten user-authorized close warm-grey fur and bare-face colors.',
 note='Static three-quarter poses. Left/right face diagonally toward the viewer; back variants turn away, with a broad visible back and receding tools.'}
local file=assert(io.open(out..'baboon-chef.json','w'));file:write(json.encode(metadata));file:close()
local report='PASS: '..count..' PNG exports; 6 saved character frames; 11 character layers; 4 station occupancies; canonical palette plus ten user-authorized character colors; binary alpha; margins; exact source and export pixels; visible capacity verified for 0 through 15 hires.'
file=assert(io.open(out..'validation.txt','w'));file:write(report..'\n');file:close();print(report)
