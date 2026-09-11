-- Run from repository root: sprite-axi run tools/art/squirrel-monkey.lua
-- Original pixel art: common squirrel monkey, informed by docs/research/squirrel-monkey-art.md.
local out='assets/Monkey/Squirrel Unpacker/'
app.fs.makeAllDirectories(out)
local W,H=64,64
local s=axi.new(W,H);s:deleteLayer(s.layers[1])
local pi=Image{fromFile='docs/references/palette.png'}
local palette,allowed=Palette(31),{}
for x=0,30 do local p=pi:getPixel(x,0);palette:setColor(x,Color(p));allowed[p]=true end
s:setPalette(palette)
local C={dark='#3d3333',shadow='#546756',fur='#87857c',light='#bcad9f',mask='#fee1b8',
 leg='#a57855',orange='#de9f47',gold='#fdd179',stem='#734c44'}
local dirs={'N','NE','E','SE','S','SW','W','NW'}
local angles={N=-90,NE=-45,E=0,SE=45,S=90}
local mirrors={NW='NE',W='E',SW='SE'}
local states={{name='idle',count=4,ms=300,loop=true},{name='dart',count=8,ms=50,loop=true},
 {name='carry',count=8,ms=50,loop=true},{name='take',count=6,ms=70,loop=false},
 {name='drop',count=6,ms=70,loop=false}}
local layers={'01 Tail behind','02 Far limbs','03 Body','04 Near limbs','05 Head','06 Tail foreground','07 Banana','08 Fingers'}
for _,name in ipairs(layers) do axi.layer(name) end
local cel,f,cs,sn,d,phase,state,bob
local function round(x) return math.floor(x+.5) end
local function P(u,v,z) return {round(32+u*cs-v*sn),round(56+(u*sn+v*cs)*.5-z)} end
local function poly(pts,color)
 local q={};for _,p in ipairs(pts) do q[#q+1]={round(p[1]),round(p[2])} end
 axi.poly(cel,q,C[color] or color,{fill=true})
end
local function line(a,b,color,w) axi.line(cel,round(a[1]),round(a[2]),round(b[1]),round(b[2]),C[color],{thickness=w or 1}) end
local function path(pts,color,w) for i=1,#pts-1 do line(pts[i],pts[i+1],color,w) end end
local function rect(x,y,w,h,c) axi.rect(cel,round(x),round(y),w,h,C[c],{fill=true}) end
local function ellipse(x,y,rx,ry,c)
 for yy=-ry,ry do local xx=math.floor(rx*math.sqrt(math.max(0,1-(yy/(ry+.35))^2)));rect(x-xx,y+yy,xx*2+1,1,c) end
end
local function layer(name) cel=axi.cel(name,f) end
local function banana(x,y)
 poly({{x-5,y-1},{x-3,y+3},{x,y+4},{x+4,y+2},{x+7,y-2},{x+7,y+2},{x+4,y+6},{x,y+7},{x-4,y+5},{x-6,y+1}},'orange')
 poly({{x-5,y-1},{x-3,y+2},{x,y+3},{x+4,y+1},{x+7,y-2},{x+5,y+3},{x+2,y+5},{x-1,y+5},{x-4,y+3}},'gold')
 path({{x-3,y+2},{x,y+3},{x+3,y+2}},'mask');line({x+7,y-3},{x+7,y-2},'stem')
end
local function head(hx,hy,rear)
 -- Large rounded ears and a split pale eye-mask; no enclosing contour.
 ellipse(hx-7,hy,2,3,'light');ellipse(hx-7,hy,1,2,'mask')
 if d~='E' then ellipse(hx+7,hy,2,3,'light');rect(hx+7,hy-1,1,3,'mask') end
 poly({{hx-6,hy-5},{hx-3,hy-7},{hx+2,hy-7},{hx+6,hy-4},{hx+7,hy+1},{hx+5,hy+5},{hx+1,hy+7},{hx-4,hy+5},{hx-7,hy+1}},'fur')
 poly({{hx-5,hy-5},{hx-2,hy-7},{hx+2,hy-6},{hx+4,hy-4},{hx,hy-3},{hx-4,hy-2}},'shadow')
 if rear then
  path({{hx-5,hy+2},{hx-3,hy+5},{hx+2,hy+5}},'light',2)
  if d=='NE' then poly({{hx+5,hy-1},{hx+7,hy},{hx+7,hy+3},{hx+5,hy+4}},'mask');rect(hx+7,hy+1,1,2,'dark') end
 elseif d=='E' then
  poly({{hx+1,hy-3},{hx+5,hy-3},{hx+7,hy},{hx+7,hy+4},{hx+3,hy+5},{hx,hy+3},{hx-1,hy}},'mask')
  rect(hx+3,hy-1,2,2,'dark');rect(hx+6,hy+2,3,3,'dark');rect(hx+5,hy+5,2,1,'light')
 else
  poly({{hx-5,hy-2},{hx-3,hy-4},{hx-1,hy-3},{hx,hy-1},{hx+2,hy-3},{hx+4,hy-3},{hx+6,hy},{hx+5,hy+4},{hx+2,hy+6},{hx-2,hy+5},{hx-5,hy+2}},'mask')
  local off=d=='SE' and 1 or 0
  rect(hx-3+off,hy-1,2,2,'dark');rect(hx+3+off,hy-1,2,2,'dark')
  poly({{hx-1+off,hy+2},{hx+2+off,hy+2},{hx+3+off,hy+4},{hx+1+off,hy+5},{hx-1+off,hy+4}},'dark')
 end
end
local all,clips,byName={},{},{}
for _,st in ipairs(states) do for row,dir in ipairs(dirs) do
 local clip={name=st.name..'_'..dir,state=st.name,direction=dir,first=#all+1,frames={},sockets={},durations={},row=row-1,loop=st.loop}
 d=mirrors[dir] or dir;local a=angles[d]*math.pi/180;cs,sn=math.cos(a),math.sin(a)
 for i=1,st.count do
  f=#all+1;if f>1 then s:newEmptyFrame() end
  state=st.name;phase=(i-1)/8*math.pi*2
  s.frames[f].duration=st.ms/1000
  local running=state=='dart' or state=='carry'
  bob=running and ({0,1,2,1,0,1,2,1})[i] or 0
  local loaded=state=='carry' or (state=='take' and i>=4) or (state=='drop' and i<=3)
  local reach=state=='take' and ({0,2,5,5,2,0})[i] or state=='drop' and ({0,2,5,5,2,0})[i] or 0
  local torsoZ=(state=='dart' and 14 or 18)+bob
  local hip=P(-5,0,torsoZ-2);local shoulder=P(4,0,torsoZ)
  local h=P(8,0,torsoZ+9)
  local rear=d=='N' or d=='NE'
  -- A long open balance tail. It never coils or carries the banana.
  layer(rear and '06 Tail foreground' or '01 Tail behind')
  local sway=running and round(math.sin(phase)*2) or (i==3 and 1 or 0)
  local tail={P(-6,0,torsoZ-3),P(-12,1,torsoZ-1),P(-19,2+sway,torsoZ+4),P(-24,5+sway,torsoZ+7),P(-28,8+sway,torsoZ+6)}
  path(tail,'fur',3);path({tail[1],tail[2],tail[3]},'light')
  path({tail[4],tail[5]},'dark',3)
  local hand
  -- Feet use alternating contact/recovery arcs. Loaded motion is a cartoon two-legged scurry.
  for side=-1,1,2 do
   layer(side==-1 and '02 Far limbs' or '04 Near limbs')
   local v=side*4;local t=phase+(side==-1 and math.pi or 0)
   local step=running and round(math.cos(t)*5) or 0
   local lift=running and round(math.max(0,math.sin(t))*4) or 0
   local root=P(-5,v,torsoZ-3);local knee=P(-7-step*.3,v+side,8+bob)
   local foot=P(-4+step,v+side,1+lift)
   path({root,knee,foot},side==-1 and 'leg' or 'orange',3)
   line(foot,{foot[1]+round(cs*3),foot[2]+round(sn*1.5)},'stem',2)
   local sh=P(4,v,torsoZ-1)
   if loaded or reach>0 then
    local palm=P(10+reach,side*3,torsoZ-3+reach*.55)
    if side==1 then hand=palm end
    path({sh,P(7,side*6,torsoZ-6),palm},side==-1 and 'leg' or 'orange',3)
   else
    local armStep=running and -step or 0
    local palm=P(8+armStep,v+side, state=='dart' and 3+lift or 7)
    path({sh,P(5+armStep*.3,v+side,9+bob),palm},side==-1 and 'leg' or 'orange',3)
    line(palm,{palm[1]+2,palm[2]},'stem',2)
   end
  end
  layer('03 Body')
  poly({{hip[1]-5,hip[2]-4},{hip[1]-2,hip[2]-7},{shoulder[1]+2,shoulder[2]-5},
   {shoulder[1]+5,shoulder[2]-1},{shoulder[1]+3,shoulder[2]+6},{hip[1]+2,hip[2]+7},{hip[1]-5,hip[2]+3}},'shadow')
  poly({{hip[1]-5,hip[2]-4},{hip[1]-2,hip[2]-7},{shoulder[1]+2,shoulder[2]-5},{shoulder[1]+3,shoulder[2]-1},
   {hip[1]+1,hip[2]+4},{hip[1]-4,hip[2]+2}},'fur')
  if not rear then path({{shoulder[1]+2,shoulder[2]+1},{shoulder[1]+1,shoulder[2]+5},{hip[1]+2,hip[2]+5}},'light',3) end
  layer('05 Head');head(h[1],h[2],rear)
  if loaded then
   -- Offset to the near side: the crescent stays readable in rear views too.
   hand=hand or P(10,3,torsoZ-3)
   layer('07 Banana');banana(hand[1]+1,hand[2]-2)
   layer('08 Fingers');rect(hand[1]-2,hand[2]-2,3,2,'leg');rect(hand[1]-1,hand[2]-2,2,1,'orange')
  end
  local socket=hand or P(10+reach,3,torsoZ-3+reach*.55)
  clip.sockets[#clip.sockets+1]={x=mirrors[dir] and 63-(socket[1]+1) or socket[1]+1,y=socket[2]-2}
  if mirrors[dir] then for _,l in ipairs(s.layers) do local c=l:cel(f);if c then
   local im=Image(W,H,ColorMode.RGB)
   for yy=0,c.image.height-1 do for xx=0,c.image.width-1 do
    local gx,gy=xx+c.position.x,yy+c.position.y
    if gx>=0 and gx<W and gy>=0 and gy<H then im:drawPixel(63-gx,gy,c.image:getPixel(xx,yy)) end
   end end
   c.image=im;c.position=Point(0,0)
  end end end
  local im=Image(W,H,ColorMode.RGB);im:drawSprite(s,f)
  all[#all+1]=im;clip.frames[#clip.frames+1]=im;clip.durations[#clip.durations+1]=st.ms
 end
 clip.last=#all;clips[#clips+1]=clip;byName[clip.name]=clip
end end
for _,c in ipairs(clips) do axi.tag(c.name,c.first,c.last,{direction='forward'}) end
axi.save(out..'squirrel-monkey.aseprite')
local meta={version=1,source='squirrel-monkey.aseprite',cell={width=W,height=H},anchor={x=32,y=56},sampling='nearest',directions=dirs,
 directionConvention='Screen compass; diagonals follow 2:1 ground axes. Reflections use x=63-x.',frameIndexBase=0,clips={}}
for _,st in ipairs(states) do
 local sheet=Image(W*st.count,H*8,ColorMode.RGB)
 for _,dir in ipairs(dirs) do local c=byName[st.name..'_'..dir]
  local entry={name=c.name,state=st.name,direction=dir,sheet='squirrel-monkey-'..st.name..'.png',loop=st.loop,durationMs=st.count*st.ms,bananaFlipX=mirrors[dir]~=nil,
   asepriteFirstFrame=c.first,asepriteLastFrame=c.last,frames={}}
  if st.name=='take' then entry.event={name='attach_banana',frame=3,timeMs=210} end
  if st.name=='drop' then entry.event={name='release_banana',frame=3,timeMs=210} end
  for i,im in ipairs(c.frames) do sheet:drawImage(im,Point((i-1)*W,c.row*H));entry.frames[#entry.frames+1]={x=(i-1)*W,y=c.row*H,w=W,h=H,durationMs=st.ms,bananaSocket=c.sockets[i]} end
  meta.clips[#meta.clips+1]=entry
 end
 sheet:saveAs(out..'squirrel-monkey-'..st.name..'.png')
end
local file=assert(io.open(out..'squirrel-monkey.json','w'));file:write(json.encode(meta));file:close()
file=assert(io.open(out..'squirrel-monkey-data.js','w'));file:write('window.SQUIRREL_MONKEY = '..json.encode(meta)..';\n');file:close()
byName.idle_SE.frames[1]:saveAs(out..'squirrel-monkey.png')
local contact=Image(512,320,ColorMode.RGB);contact:clear(app.pixelColor.rgba(164,197,175,255))
for row,st in ipairs(states) do for col,dir in ipairs(dirs) do contact:drawImage(byName[st.name..'_'..dir].frames[st.name=='take' and 4 or 1],Point((col-1)*64,(row-1)*64)) end end
contact:saveAs(out..'squirrel-monkey-contact.png')
-- Enlarged pose study with unscaled reference worker in the same native scene.
local native=Image(320,100,ColorMode.RGB);native:clear(app.pixelColor.rgba(164,197,175,255))
native:drawImage(Image{fromFile='docs/references/spider-worker.png'},Point(7,20))
for i,name in ipairs({'idle_SE','dart_SE','carry_SE','take_SE'}) do native:drawImage(byName[name].frames[name=='take_SE' and 4 or 1],Point(64+(i-1)*60,20)) end
local big=Image(960,300,ColorMode.RGB)
for y=0,299 do for x=0,959 do big:drawPixel(x,y,native:getPixel(math.floor(x/3),math.floor(y/3))) end end
big:saveAs(out..'squirrel-monkey-preview.png')
-- Standalone fruit uses the same crescent and origin as every carry frame.
local fruitSprite=Sprite(24,24,ColorMode.RGB);cel=fruitSprite:newCel(fruitSprite.layers[1],1,Image(24,24,ColorMode.RGB),Point(0,0));banana(10,10)
local fruit=Image(24,24,ColorMode.RGB);fruit:drawSprite(fruitSprite,1);fruit:saveAs(out..'banana-transfer.png');fruitSprite:close()
app.activeSprite=s;axi.save(out..'squirrel-monkey.aseprite')
print('Rendered '..#all..' frames / '..#clips..' clips')
