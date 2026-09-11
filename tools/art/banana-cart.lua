-- From the repository root: sprite-axi run tools/art/banana-cart.lua
-- Requires ASEPRITE_BIN. Pixel-authored seated bodies, canonical unscaled heads.
local out='assets/BananaCart/'
app.fs.makeAllDirectories(out)
local W,H,AX,AY=208,176,104,126
local dirs={'N','NE','E','SE','S','SW','W','NW'}
local angles={-90,-45,0,45,90,135,180,225}
local C={ink='#3d3333',fur='#593e47',furLight='#7a5859',skin='#734c44',tan='#bcad9f',
 wood='#a57855',woodLight='#d4c692',woodDark='#734c44',woodEnd='#87857c',
 gold='#fdd179',goldShade='#de9f47',cream='#fee1b8',stem='#819447',shadow='#546756'}
local function round(x) return math.floor(x+.5) end
local source=app.open('assets/Monkey/Spider Worker/spider_monkey_directional_walks.aseprite')
local heads={}
for di,d in ipairs(dirs) do
 local body
 for _,l in ipairs(source.layers) do if l.name=='Body' then body=l:cel((di-1)*12+1) end end
 -- Only the head is copied. The seated torso, arms, legs and tail are drawn below.
 local im=Image(64,64,ColorMode.RGB)
 for y=0,27 do for x=0,63 do
  local lx,ly=x-body.position.x,y-body.position.y
  if lx>=0 and ly>=0 and lx<body.image.width and ly<body.image.height then
   im:drawPixel(x,y,body.image:getPixel(lx,ly))
  end
 end end
 heads[d]=im
end
source:close()
local bananaSource=app.open('assets/Banana/banana-bunch.aseprite')
for _,l in ipairs(bananaSource.layers) do if l.name=='01 Ground shadow' then l.isVisible=false end end
local fruit=Image(48,48,ColorMode.RGB);fruit:drawSprite(bananaSource,1);bananaSource:close()
local s=axi.new(W,H);s:deleteLayer(s.layers[1])
local pi=Image{fromFile='docs/references/palette.png'}
local palette,allowed=Palette(31),{}
for x=0,30 do local p=pi:getPixel(x,0);palette:setColor(x,Color(p));allowed[p]=true end
s:setPalette(palette)
local cel,cs,sn,frame,di,d
local function point(u,v,z) return {round(AX+u*cs-v*sn),round(AY+(u*sn+v*cs)*.5-z)} end
local function poly(pts,color)
 local q={};for _,p in ipairs(pts) do q[#q+1]={round(p[1]),round(p[2])} end
 axi.poly(cel,q,C[color] or color,{fill=true})
end
local function path(pts,color,width)
 for i=1,#pts-1 do axi.line(cel,round(pts[i][1]),round(pts[i][2]),round(pts[i+1][1]),round(pts[i+1][2]),C[color] or color,{thickness=width or 1}) end
end
local function line3(a,b,color,width) path({point(table.unpack(a)),point(table.unpack(b))},color,width) end
local function face(pts,color) local q={};for _,p in ipairs(pts) do q[#q+1]=point(table.unpack(p)) end;poly(q,color) end
local function setLayer(name) cel=axi.cel(d..' / '..name,frame) end
local function box(u0,u1,v0,v1,z0,z1)
 face({{u0,v0,z0},{u1,v0,z0},{u1,v1,z0},{u0,v1,z0}},'woodDark')
 local u=sn>0 and u1 or u0
 face({{u,v0,z0},{u,v1,z0},{u,v1,z1},{u,v0,z1}},'woodDark')
 local v=cs>0 and v1 or v0
 face({{u0,v,z0},{u1,v,z0},{u1,v,z1},{u0,v,z1}},'wood')
 face({{u0,v0,z1},{u1,v0,z1},{u1,v1,z1},{u0,v1,z1}},'woodLight')
end
local function wheel(u,v,phase)
 local pts={};local outer=v+(v>0 and 2 or -2)
 for i=0,23 do local a=i*math.pi/12;pts[#pts+1]=point(u+11*math.cos(a),outer,11+11*math.sin(a)) end
 poly(pts,'woodDark')
 pts={};for i=0,23 do local a=i*math.pi/12;pts[#pts+1]=point(u+9*math.cos(a),outer+(v>0 and 1 or -1),11+9*math.sin(a)) end
 poly(pts,'wood')
 -- Solid wooden wheels with broad grain/spokes, no metal rim or tyre.
 for i=0,3 do
  -- Forward is +u: a point at ground contact must move toward -u.
  local a=-phase*2*math.pi+i*math.pi/2
  line3({u,outer,11},{u+8*math.cos(a),outer,11+8*math.sin(a)},i%2==0 and 'woodLight' or 'woodDark',2)
 end
 local hub=point(u,outer,11);axi.ellipse(cel,hub[1],hub[2],2,2,C.woodDark,{fill=true})
 path({{hub[1],hub[2]-1},{hub[1]+1,hub[2]-1}},'woodLight')
end
local function chassis()
 box(-63,54,-27,27,16,19)
 -- Two shallow seams and one repaired corner keep the deck quiet.
 line3({-60,-9,19},{51,-9,19},'wood',1);line3({-60,10,19},{51,10,19},'wood',1)
 box(43,51,-25,-17,19,20)
 for _,u in ipairs({-48,40}) do line3({u,-30,11},{u,30,11},'woodDark',3) end
end
local headCenters={N=32,NE=39,E=40,SE=40,S=32,SW=23,W=23,NW=24}
local function monkey(u,v,driver,phase,identity)
 local p=point(u,v,27);local x,y=p[1],p[2]
 local f={cs,sn*.5};local side={-sn,cs*.5}
 local bounce=driver and 0 or round(math.sin(phase*math.pi*2)*.55)
 local hx,hy=x+round(cs*4),y-23+bounce
 -- A sturdy stool is visible below the bent knees.
 box(u-6,u+5,v-7,v+7,21,24)
 line3({u-3,v-5,19},{u-3,v-5,23},'woodDark',2)
 local sign=v>0 and 1 or -1
 if driver then sign=sn<0 and 1 or -1 end
 -- Authored high, open curl: three distinct silhouettes with restrained tips.
 local tx=x+round(side[1]*sign*10)-round(cs*7)
 local ty=y-34-round(math.abs(side[2])*9)
 local curl={{x-2,y-2},{x-round(cs*8),y-5},{tx,ty+17},{tx-2,ty+8},
  {tx-3,ty+2},{tx-1,ty-3},{tx+3,ty-5},{tx+7,ty-4},{tx+10,ty},
  {tx+9,ty+5},{tx+6,ty+7},{tx+3,ty+5},{tx+3,ty+2},{tx+5,ty+1}}
 path(curl,'ink',3);path(curl,'fur',1)
 path({{tx-1,ty-2},{tx+3,ty-4},{tx+6,ty-3}},'furLight')
 -- Knees and feet follow real circular pedal endpoints in the wheel plane.
 for _,sideSign in ipairs({-1,1}) do
  local sv=v+sideSign*5
  local a=-phase*2*math.pi+(sideSign==1 and math.pi or 0)
  local pu=u+12+(driver and 0 or math.cos(a)*5)
  local pz=driver and 19 or 21+math.sin(a)*5
  local hip=point(u,sv,27);local knee=point(u+9,sv,32+(driver and 0 or math.sin(a)*2))
  local foot=point(pu,sv,pz)
  if not driver then
   line3({u+12,sv,21},{pu,sv,pz},'woodDark',2)
   line3({pu-3,sv,pz},{pu+3,sv,pz},'woodLight',2)
  end
  path({hip,knee,foot},'ink',4);path({hip,knee,foot},'fur',2)
  path({foot,{foot[1]+round(cs*4),foot[2]+round(sn*2)}},'skin',2)
 end
 -- Seated torso retains the worker's narrow shoulders and rounded haunch.
 poly({{x-7,y-3},{x-7,y-13},{hx-5,hy+4},{hx+5,hy+5},{x+7,y-13},{x+6,y-2},{x+2,y+2},{x-4,y+1}},'ink')
 poly({{x-5,y-5},{x-5,y-13},{hx-3,hy+6},{hx+2,hy+7},{x+3,y-6},{x,y},{x-3,y-1}},'fur')
 path({{x-4,y-12},{hx-3,hy+7},{hx-1,hy+6}},'furLight',2)
 -- Canonical head pixels, translated only; no rescaling or rotated billboard.
 cel.image:drawImage(heads[d],Point(hx-headCenters[d]-cel.position.x,hy-20-cel.position.y))
 -- A wooden tiller for the driver, fixed handgrips for both pedal passengers.
 local handleU=u+(driver and 13 or 10)
 line3({handleU,v,19},{handleU,v,35},'woodDark',2)
 line3({handleU,v-9,35},{handleU,v+9,35},'woodLight',2)
 for _,ss in ipairs({-1,1}) do
  local sh={hx+round(side[1]*ss*5),hy+8+round(side[2]*ss*5)}
  local hand=point(handleU,v+ss*7,35)
  local elbow={round((sh[1]+hand[1])/2)-round(cs*3),round((sh[2]+hand[2])/2)+5}
  path({sh,elbow,hand},'ink',3);path({sh,elbow,hand},'fur',1)
  path({hand,{hand[1]+1,hand[2]}},'skin',2)
 end
end
local payload={{-50,-15,24},{-34,-14,25},{-50,11,25},{-34,12,26},{-43,-4,37},{-42,9,43}}
local function cargo(state,t)
 local tilt=0
 if state=='offload' then
  tilt=t<5 and t/5*.25 or t<17 and .25 or math.max(0,(22-t)/5)*.25
 end
 local function bp(u,v,z)
  return point(-63+(u+63)*math.cos(tilt)-(z-19)*math.sin(tilt),v,
   19+(u+63)*math.sin(tilt)+(z-19)*math.cos(tilt))
 end
 local function bface(pts,color) local q={};for _,p in ipairs(pts) do q[#q+1]=bp(table.unpack(p)) end;poly(q,color) end
 local function bline(a,b,color,width) path({bp(table.unpack(a)),bp(table.unpack(b))},color,width) end
 local function wall(v)
  bface({{-63,v,20},{-20,v,20},{-20,v,35},{-35,v,35},{-49,v,34},{-63,v,35}},'wood')
  bline({-62,v,34},{-21,v,35},'woodLight',2)
  bline({-59,v,27},{-28,v,27},'woodDark')
  for _,u in ipairs({-61,-22}) do bline({u,v,20},{u,v,37},'woodDark',3);bline({u,v,21},{u,v,36},'woodLight') end
 end
 local function endWall(u,gate)
  local open=gate and (tilt/.25) or 0
  local upperU=u-open*14;local upperZ=35-open*14
  bface({{u,-26,20},{u,26,20},{upperU,26,upperZ},{upperU,-26,upperZ}},'woodDark')
  bline({upperU,-26,upperZ},{upperU,26,upperZ},'woodLight',2)
 end
 bface({{-63,-26,20},{-20,-26,20},{-20,26,20},{-63,26,20}},'wood')
 wall(cs>=0 and -26 or 26)
 endWall(sn>=0 and -63 or -20,sn>=0)
 local bunches={}
 for i,p in ipairs(payload) do
  local visible=state=='travel_full' or state=='offload'
  local drop,slide=0,0
  if state=='fill' then
   local start=1+(i-1)*3;visible=t>=start
   if visible then drop=({24,10,-1,0})[math.min(4,t-start+1)] end
  elseif state=='offload' then
   -- Upper bunches leave first, so the pile does not hang without support.
   local start=5+(6-i)*2
   if t>=start then slide=(t-start)*10;visible=slide<40 end
  end
  if visible then bunches[#bunches+1]={p=p,drop=drop,slide=slide,depth=p[1]*sn+p[2]*cs} end
 end
 table.sort(bunches,function(a,b)return a.depth<b.depth end)
 for _,b in ipairs(bunches) do
  local descend=math.min(b.p[3]-12,b.slide*.8)
  local p=bp(b.p[1]-b.slide,b.p[2],b.p[3]+b.drop-descend)
  cel.image:drawImage(fruit,Point(p[1]-24-cel.position.x,p[2]-34-cel.position.y))
 end
 wall(cs>=0 and 26 or -26)
 endWall(sn>=0 and -20 or -63,sn<0)
end
local states={{name='travel_empty',count=12,ms=80,loop=true},{name='fill',count=24,ms=100,loop=false},
 {name='travel_full',count=12,ms=80,loop=true},{name='offload',count=24,ms=100,loop=false}}
local clips,byName,allFrames={}, {}, {}
frame=0
for ddi,dd in ipairs(dirs) do
 di,d=ddi,dd;cs=math.cos(math.rad(angles[di]));sn=math.sin(math.rad(angles[di]))
 -- Components get a per-view depth stack, retaining separately editable crew.
 local actors={{name='Driver',u=35,v=0,driver=true},{name='Pedaller left',u=1,v=-17},
  {name='Pedaller right',u=-3,v=17},{name='Banana bed and cargo',u=-43,v=0,cargo=true}}
 table.sort(actors,function(a,b) return a.u*sn+a.v*cs < b.u*sn+b.v*cs end)
 for _,name in ipairs({'Ground shadow','Far wooden wheels','Deck and axles'}) do axi.layer(d..' / '..name) end
 for _,a in ipairs(actors) do axi.layer(d..' / '..a.name) end
 axi.layer(d..' / Near wooden wheels')
 for _,state in ipairs(states) do
  local clip={name=state.name..'_'..d,state=state.name,direction=d,first=frame+1,frames={},durations={},loop=state.loop,row=di-1}
  for t=0,state.count-1 do
   frame=frame+1;if frame>1 then s:newEmptyFrame() end
   s.frames[frame].duration=state.ms/1000
   local travel=state.loop;local phase=travel and t/12 or 0
   setLayer('Ground shadow')
   local sh={};for j=0,31 do local a=j*math.pi/16;sh[#sh+1]=point(math.cos(a)*68,math.sin(a)*33,0) end
   poly(sh,'shadow')
   setLayer('Far wooden wheels');local far=cs>=0 and -30 or 30
   wheel(-48,far,phase);wheel(40,far,phase)
   setLayer('Deck and axles');chassis()
   for _,a in ipairs(actors) do
    setLayer(a.name)
    if a.cargo then cargo(state.name,t)
    else monkey(a.u,a.v,a.driver,a.driver and 0 or (phase+(a.v>0 and .5 or 0)),a.name) end
   end
   setLayer('Near wooden wheels');wheel(-48,-far,phase);wheel(40,-far,phase)
   local im=Image(W,H,ColorMode.RGB);im:drawSprite(s,frame)
   clip.frames[#clip.frames+1]=im;clip.durations[#clip.durations+1]=state.ms;allFrames[frame]=im
  end
  clip.last=frame;clips[#clips+1]=clip;byName[clip.name]=clip
 end
 print('Rendered '..d)
end
for _,clip in ipairs(clips) do axi.tag(clip.name,clip.first,clip.last,{direction='forward'}) end
axi.save(out..'banana-cart.aseprite')
local function same(a,b)
 for y=0,H-1 do for x=0,W-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end
 return true
end
local bounds={W,H,0,0};local colors={}
for _,im in ipairs(allFrames) do for y=0,H-1 do for x=0,W-1 do
 local p=im:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
 assert(a==0 or a==255,'Partial alpha')
 if a>0 then
  assert(allowed[p],'Palette drift');assert(x>0 and x<W-1 and y>0 and y<H-1,'Clipped pixel')
  colors[p]=true;bounds={math.min(bounds[1],x),math.min(bounds[2],y),math.max(bounds[3],x),math.max(bounds[4],y)}
 end
end end end
for _,dd in ipairs(dirs) do
 local empty,full,fill,off=byName['travel_empty_'..dd].frames,byName['travel_full_'..dd].frames,byName['fill_'..dd].frames,byName['offload_'..dd].frames
 assert(same(empty[1],fill[1]),'Empty to fill seam '..dd)
 assert(same(fill[#fill],full[1]),'Fill to full seam '..dd)
 assert(same(full[1],off[1]),'Full to offload seam '..dd)
 assert(same(off[#off],empty[1]),'Offload to empty seam '..dd)
 assert(not same(empty[1],empty[4]),'No travel motion '..dd)
end
local disk=app.open(out..'banana-cart.aseprite')
assert(#disk.frames==frame and #disk.tags==32,'Saved frame/tag count')
for _,clip in ipairs(clips) do
 local tag;for _,t in ipairs(disk.tags) do if t.name==clip.name then tag=t end end
 assert(tag and tag.fromFrame.frameNumber==clip.first and tag.toFrame.frameNumber==clip.last,'Saved tag bounds')
 for i,im in ipairs(clip.frames) do
  local check=Image(W,H,ColorMode.RGB);check:drawSprite(disk,clip.first+i-1)
  assert(same(im,check),'Saved master pixels');assert(round(disk.frames[clip.first+i-1].duration*1000)==clip.durations[i],'Saved timing')
 end
end
disk:close();app.activeSprite=s
local metadata={version=1,source='banana-cart.aseprite',cell={width=W,height=H},anchor={x=AX,y=AY},
 directions=dirs,directionConvention='Screen compass; N up, E right; diagonals use 2:1 ground axes.',
 sampling='nearest',frameIndexBase=0,crew={driver=1,pedallers=2},clips={}}
for _,state in ipairs(states) do
 local sheet=Image(W*state.count,H*8,ColorMode.RGB)
 for _,dd in ipairs(dirs) do
  local clip=byName[state.name..'_'..dd]
  local entry={name=clip.name,state=state.name,direction=dd,sheet='banana-cart-'..state.name..'.png',loop=state.loop,
   asepriteFirstFrame=clip.first,asepriteLastFrame=clip.last,durationMs=state.count*state.ms,frames={}}
  for i,im in ipairs(clip.frames) do
   sheet:drawImage(im,Point((i-1)*W,clip.row*H))
   entry.frames[#entry.frames+1]={x=(i-1)*W,y=clip.row*H,w=W,h=H,durationMs=state.ms}
  end
  metadata.clips[#metadata.clips+1]=entry
 end
 local name=out..'banana-cart-'..state.name..'.png';sheet:saveAs(name)
 local check=Image{fromFile=name}
 for y=0,sheet.height-1 do for x=0,sheet.width-1 do assert(check:getPixel(x,y)==sheet:getPixel(x,y),'PNG export mismatch') end end
end
local file=assert(io.open(out..'banana-cart.json','w'));file:write(json.encode(metadata));file:close()
-- Review-only native grid: clockwise compass across columns; lifecycle down rows.
local contact=Image(W*8,H*4,ColorMode.RGB);contact:clear(app.pixelColor.rgba(164,197,175,255))
for row,state in ipairs(states) do for col,dd in ipairs(dirs) do
 local clip=byName[state.name..'_'..dd];local i=state.name=='fill' and 13 or state.name=='offload' and 12 or 1
 contact:drawImage(clip.frames[i],Point((col-1)*W,(row-1)*H))
end end
contact:saveAs(out..'banana-cart-contact.png')
local worker=Image{fromFile='docs/references/spider-worker.png'}
local preview=Sprite(640,400,ColorMode.RGB);preview:setPalette(palette)
local function review(im)
 local native=Image(320,200,ColorMode.RGB);native:clear(app.pixelColor.rgba(164,197,175,255))
 native:drawImage(worker,Point(10,109));native:drawImage(im,Point(85,34))
 local big=Image(640,400,ColorMode.RGB)
 for y=0,399 do for x=0,639 do big:drawPixel(x,y,native:getPixel(math.floor(x/2),math.floor(y/2))) end end
 return big
end
local n=0
for _,state in ipairs(states) do
 local clip=byName[state.name..'_SE']
 local repeats=state.loop and 2 or 1
 for rep=1,repeats do for i,im in ipairs(clip.frames) do
  n=n+1;if n>1 then preview:newEmptyFrame() end
  preview.frames[n].duration=clip.durations[i]/1000
  preview:newCel(preview.layers[1],n,review(im),Point(0,0))
 end end
end
preview:saveCopyAs(out..'banana-cart-lifecycle.gif')
review(byName.travel_full_SE.frames[1]):saveAs(out..'banana-cart-preview.png')
preview:close()
app.activeSprite=s;axi.save(out..'banana-cart.aseprite')
local cc=0;for _ in pairs(colors) do cc=cc+1 end
print(string.format('PASS: %d frames; 32 clips; %d canonical colors; binary alpha; bounds [%d,%d,%d,%d]; all 32 lifecycle seams exact; saved master/tags/timings and PNGs agree.',frame,cc,table.unpack(bounds)))
