-- Run: sprite-axi run tools/art/verify-squirrel-monkey.lua -f 'assets/Monkey/Squirrel Unpacker/squirrel-monkey.aseprite'
local out='assets/Monkey/Squirrel Unpacker/'
local s=app.activeSprite
local file=assert(io.open(out..'squirrel-monkey.json','r'));local meta=json.decode(file:read('*a'));file:close()
assert(s.width==64 and s.height==64 and #s.frames==256 and #s.tags==40,'Master layout')
assert(meta.anchor.x==32 and meta.anchor.y==56 and #meta.clips==40,'Metadata layout')
local pi=Image{fromFile='docs/references/palette.png'};local allowed={}
for x=0,30 do allowed[pi:getPixel(x,0)]=true end
local frames,byName,sheets,colors={},{},{},{}
local transfer=Image{fromFile=out..'banana-transfer.png'}
local bounds={64,64,0,0}
local function same(a,b)
 for y=0,63 do for x=0,63 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end;return true
end
local bananaLayer
for _,l in ipairs(s.layers) do if l.name=='07 Banana' then bananaLayer=l end end
assert(bananaLayer,'Editable banana layer')
for _,c in ipairs(meta.clips) do
 byName[c.name]=c
 local tag;for _,t in ipairs(s.tags) do if t.name==c.name then tag=t end end
 assert(tag and tag.fromFrame.frameNumber==c.asepriteFirstFrame and tag.toFrame.frameNumber==c.asepriteLastFrame,'Tag bounds')
 sheets[c.sheet]=sheets[c.sheet] or Image{fromFile=out..c.sheet}
 local sheet=sheets[c.sheet];assert(sheet.width==#c.frames*64 and sheet.height==512,'Sheet size')
 local total=0
 for i,r in ipairs(c.frames) do
  local f=c.asepriteFirstFrame+i-1;local im=Image(64,64,ColorMode.RGB);im:drawSprite(s,f);frames[f]=im
  local ms=math.floor(s.frames[f].duration*1000+.5);assert(ms==r.durationMs,'Timing');total=total+ms
  assert(r.bananaSocket.x>=0 and r.bananaSocket.x<64 and r.bananaSocket.y>=0 and r.bananaSocket.y<64,'Socket bounds')
  for y=0,63 do for x=0,63 do
   local p=im:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
   assert(p==sheet:getPixel(r.x+x,r.y+y),'PNG/master mismatch frame '..f)
   assert(a==0 or a==255,'Partial alpha')
   if a>0 then assert(allowed[p],'Palette drift');assert(x>0 and x<63 and y>0 and y<63,'Clipped frame '..f..' at '..x..','..y)
    colors[p]=true;bounds={math.min(bounds[1],x),math.min(bounds[2],y),math.max(bounds[3],x),math.max(bounds[4],y)}
   end
  end end
  local shouldCarry=c.state=='carry' or (c.state=='take' and i>=4) or (c.state=='drop' and i<=3)
  local bc=bananaLayer:cel(f);assert((bc~=nil)==shouldCarry,'Banana ownership frame '..f)
  if bc then local count=0
   for y=0,bc.image.height-1 do for x=0,bc.image.width-1 do if app.pixelColor.rgbaA(bc.image:getPixel(x,y))>0 then count=count+1 end end end
   assert(count>=40,'Fruit silhouette')
   local expected=Image(64,64,ColorMode.RGB)
   local socket=r.bananaSocket
   for y=0,23 do for x=0,23 do
    local gx=socket.x+(c.bananaFlipX and 10-x or x-10)
    local gy=socket.y+y-10
    if gx>=0 and gx<64 and gy>=0 and gy<64 then expected:drawPixel(gx,gy,transfer:getPixel(x,y)) end
   end end
   local actual=Image(64,64,ColorMode.RGB);actual:drawImage(bc.image,bc.position)
   assert(same(actual,expected),'Transfer socket/flip registration '..f)
  end
 end
 assert(total==c.durationMs,'Clip total')
 if c.state=='take' or c.state=='drop' then assert(c.event.frame==3 and c.event.timeMs==210,'Transfer timing') end
 if c.state=='dart' or c.state=='carry' then assert(not same(frames[c.asepriteFirstFrame],frames[c.asepriteFirstFrame+2]),'Missing gait motion') end
end
for _,state in ipairs({'idle','dart','carry','take','drop'}) do for _,pair in ipairs({{'NE','NW'},{'E','W'},{'SE','SW'}}) do
 local a,b=byName[state..'_'..pair[1]],byName[state..'_'..pair[2]]
 for i=0,#a.frames-1 do local aa,bb=frames[a.asepriteFirstFrame+i],frames[b.asepriteFirstFrame+i]
  for y=0,63 do for x=0,63 do assert(aa:getPixel(x,y)==bb:getPixel(63-x,y),'Mirror mismatch') end end
 end
end end
-- The wraparound must be no larger than the largest ordinary adjacent change.
local function diff(a,b) local n=0;for y=0,63 do for x=0,63 do if a:getPixel(x,y)~=b:getPixel(x,y) then n=n+1 end end end;return n end
for _,c in ipairs(meta.clips) do if c.loop then
 local maximum=0;for f=c.asepriteFirstFrame,c.asepriteLastFrame-1 do maximum=math.max(maximum,diff(frames[f],frames[f+1])) end
 assert(diff(frames[c.asepriteLastFrame],frames[c.asepriteFirstFrame])<=maximum,'Loop snap '..c.name)
end end
local cc=0;for _ in pairs(colors) do cc=cc+1 end
print(string.format('PASS: 256 master/PNG frames; 40 tags; timings; sockets; ownership events; mirrors; loop boundaries; %d canonical colors; binary alpha; bounds [%d,%d,%d,%d].',cc,table.unpack(bounds)))
