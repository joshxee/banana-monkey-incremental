-- Run from repository root: sprite-axi run tools/art/ground.lua
-- Corner-Wang terrain. Shared corner values determine every edge; variants
-- change only authored interior marks, never the boundary silhouette.
local out='assets/Ground/'
app.fs.makeAllDirectories(out..'tiles')
local W,H,V=128,64,4
local pi=math.pi
local colors={jungle='#a4c5af',dirt='#bcad9f',moss='#89a477',soil='#a57855'}
local rgba={}
for k,v in pairs(colors) do rgba[k]=Color{r=tonumber(v:sub(2,3),16),g=tonumber(v:sub(4,5),16),b=tonumber(v:sub(6,7),16),a=255}.rgbaPixel end
local palImage=Image{fromFile='docs/references/palette.png'}
local pal,allowed=Palette(31),{}
for x=0,30 do local p=palImage:getPixel(x,0);pal:setColor(x,Color(p));allowed[p]=true end
local function corner(m,b) return (m & b)>0 and 1 or -1 end
local function field(m,u,v)
 -- Piecewise linear marching contours avoid the repeated semicircles of
 -- bilinear corner interpolation. A shallow interior warp breaks the edge
 -- into organic runs without moving shared crossings or corner values.
 local a=u+.023*math.sin(3*pi*v)*math.sin(pi*u)
 local b=v+.018*math.sin(4*pi*u)*math.sin(pi*v)
 local single={ [1]=1-2*(a+b),[2]=2*(a-b)-1,
   [4]=2*(a+b)-3,[8]=2*(b-a)-1 }
 local f
 if m==0 then f=-1 elseif m==15 then f=1
 elseif single[m] then f=single[m]
 elseif single[15-m] then f=-single[15-m]
 elseif m==3 then f=1-2*b elseif m==6 then f=2*a-1
 elseif m==12 then f=2*b-1 elseif m==9 then f=1-2*a
 else
  -- Opposite-corner saddle: dirt stays connected through the center.
  f=corner(m,1)*(1-a)*(1-b)+corner(m,2)*a*(1-b)
    +corner(m,4)*a*b+corner(m,8)*(1-a)*b
    +.16*math.sin(pi*u)*math.sin(pi*v)
 end
 return math.max(-1,math.min(1,f))
end
local function uv(x,y) return (x+.5-64)/128+(y+.5)/64, -(x+.5-64)/128+(y+.5)/64 end
local function inside(u,v) return u>=0 and v>=0 and u<1 and v<1 end
local function edgeDistance(u,v) return math.min(u,v,1-u,1-v) end
local function new(w,h)
 local s=axi.new(w,h);s:deleteLayer(s.layers[1]);s:setPalette(pal);return s
end
-- Four different, deliberately sparse clusters. Large areas remain empty.
-- Each polygon is a flattened leaf fragment or a compacted earthen scuff.
local motifs={
 {},
 {{{61,15},{68,16},{72,18},{66,18}},{{46,41},{52,39},{58,40},{53,42}}},
 {{{69,29},{76,26},{84,27},{89,26},{95,28},{88,31},{79,31},{74,33},{65,32}},{{60,35},{65,34},{68,35},{63,36}}},
 {{{40,27},{50,24},{57,25},{53,27},{46,28}},{{52,29},{59,28},{65,29},{60,30}}}
}
local function inPoly(x,y,p)
 local c=false;local j=#p
 for i=1,#p do
  if (p[i][2]>y)~=(p[j][2]>y) and x<(p[j][1]-p[i][1])*(y-p[i][2])/(p[j][2]-p[i][2])+p[i][1] then c=not c end
  j=i
 end
 return c
end
local atlas=new(1024,512)
local base=axi.cel('01 Quiet terrain fields',1)
local fringe=axi.cel('02 Broken moss at dirt boundary',1)
local detail=axi.cel('03 Sparse leaf fragments and soil scuffs',1)
local tiles={}
local data=assert(io.open(out..'ground-atlas.json','w'))
data:write('{\n "image":"ground-atlas.png", "master":"ground.aseprite",\n "tileWidth":128,"tileHeight":64,"columns":8,"variants":4,\n "anchor":[64,32],"stepU":[64,32],"stepV":[-64,32],\n "cornerBits":{"top":1,"right":2,"bottom":4,"left":8},\n "bitMeaning":"1=dirt, 0=jungle", "saddleConnection":"dirt",\n "sampling":"nearest; pixel-aligned; no mipmaps",\n "tiles":[\n')
for m=0,15 do
 tiles[m]={}
 for variant=0,V-1 do
  local idx=m*V+variant;local ox,oy=(idx%8)*W,math.floor(idx/8)*H
  local img=Image(W,H,ColorMode.RGB)
  local opaque,marks=0,0
  for y=0,H-1 do for x=0,W-1 do
   local u,v=uv(x,y)
   if inside(u,v) then
    local f=field(m,u,v);local dirt=f>0
    local p=dirt and rgba.dirt or rgba.jungle
    base.image:putPixel(ox+x,oy+y,p)
    -- Discontinuous green incursions, not a dark contour or raised rim.
    -- Silence the final edge strip so every variant shares identical joins.
    if m~=0 and m~=15 and edgeDistance(u,v)>.045 and f<.055 and f>-.19 then
     local patches=math.sin(u*19+v*11)+math.sin(v*31-u*7)
     if patches>.65 and (f<0 or patches>1.25) then p=rgba.moss;fringe.image:putPixel(ox+x,oy+y,p) end
    end
    if edgeDistance(u,v)>.14 and math.abs(f)>.28 then
     for _,poly in ipairs(motifs[variant+1]) do
      if inPoly(x+.5,y+.5,poly) then
       -- Dirt marks have only a thin swept edge. Jungle fragments are small
       -- flat shapes; neither receives an enclosing outline or highlight.
       if not dirt or y%4<2 then
        p=dirt and rgba.soil or rgba.moss
        detail.image:putPixel(ox+x,oy+y,p);marks=marks+1
       end
      end
     end
    end
    assert(allowed[p],'Ground palette drift')
    img:putPixel(x,y,p);opaque=opaque+1
   end
  end end
  assert(opaque==4096,'Diamond coverage must be exactly 4096 pixels')
  assert(marks/opaque<.035,'Texture competes with subjects')
  local name=string.format('ground-%02d-v%d',m,variant)
  img:saveAs(out..'tiles/'..name..'.png');tiles[m][variant]=img
  data:write(string.format('  {"mask":%d,"variant":%d,"x":%d,"y":%d,"w":128,"h":64,"file":"tiles/%s.png"}%s\n',m,variant,ox,oy,name,idx<63 and ',' or ''))
 end
end
data:write(' ]\n}\n');data:close()
axi.save(out..'ground.aseprite');atlas:saveCopyAs(out..'ground-atlas.png')
tiles[0][2]:saveAs(out..'jungle-floor.png');tiles[15][2]:saveAs(out..'dirt-floor.png')
local flat=Image{fromFile=out..'ground-atlas.png'}
for m=0,15 do for variant=0,V-1 do
 local idx=m*V+variant;local ox,oy=(idx%8)*W,math.floor(idx/8)*H
 for y=0,H-1 do for x=0,W-1 do
  local p=tiles[m][variant]:getPixel(x,y)
  assert(flat:getPixel(ox+x,oy+y)==p,'Atlas/master/tile disagreement')
  local a=app.pixelColor.rgbaA(p);assert(a==0 or a==255,'Partial alpha')
  local u,v=uv(x,y)
  if inside(u,v) and edgeDistance(u,v)<.045 then assert(p==tiles[m][0]:getPixel(x,y),'Variant changed join') end
 end end
end end
-- Exhaustive shared-edge field checks for all compatible mask pairs.
local joins=0
for a=0,15 do for b=0,15 do
 if corner(a,2)==corner(b,1) and corner(a,4)==corner(b,8) then
  for t=0,128 do assert(math.abs(field(a,1,t/128)-field(b,0,t/128))<1e-10,'U edge mismatch') end
  joins=joins+1
 end
 if corner(a,8)==corner(b,1) and corner(a,4)==corner(b,2) then
  for t=0,128 do assert(math.abs(field(a,t/128,1)-field(b,t/128,0))<1e-10,'V edge mismatch') end
  joins=joins+1
 end
end end
assert(joins==128)
-- Context proof: a winding jungle margin, a town clearing and a dirt spur.
local PW,PH=960,640
local preview=new(PW,PH)
local ground=axi.cel('01 Assembled ground - native resolution',1)
local coverage={}
local function terrain(i,j)
 local x,y=480+64*(i-j),32*(i+j)
 local boundary=420+52*math.sin(y/105)+22*math.sin(y/43)
 local spur=y>405 and y<505 and x>170
 return x>boundary or spur
end
local function mask(i,j)
 return (terrain(i,j) and 1 or 0)+(terrain(i+1,j) and 2 or 0)
   +(terrain(i+1,j+1) and 4 or 0)+(terrain(i,j+1) and 8 or 0)
end
local cells={}
for i=-12,26 do for j=-12,26 do
 local m=mask(i,j)
 local hash=((i*73856093) ~ (j*19349663)) & 0xffffffff
 hash=((hash ~ (hash >> 13))*1274126177) & 0xffffffff
 local variant=(hash ~ (hash >> 16))%4
 local x,y=416+64*(i-j),32*(i+j)
 if x<PW and y<PH and x+W>0 and y+H>0 then
  local img=tiles[m][variant];ground.image:drawImage(img,Point(x,y))
  cells[#cells+1]={i=i,j=j,mask=m,variant=variant,x=x,y=y}
  for py=0,H-1 do for px=0,W-1 do
   local xx,yy=x+px,y+py
   if xx>=0 and yy>=0 and xx<PW and yy<PH and app.pixelColor.rgbaA(img:getPixel(px,py))>0 then
    local key=yy*PW+xx;coverage[key]=(coverage[key] or 0)+1
   end
  end end
 end
end end
for k=0,PW*PH-1 do assert(coverage[k]==1,'Tiling gap or overlap at pixel '..k) end
ground.image:saveAs(out..'ground-assembled.png')
local refs=axi.cel('02 Existing vegetation - review only',1)
local function prop(path,footX,footY)
 local img=Image{fromFile=path};refs.image:drawImage(img,Point(footX-160,footY-316))
end
prop('assets/Jungle/jungle-leaning.png',145,301)
prop('assets/Jungle/banana-fruiting.png',355,322)
prop('assets/Jungle/jungle-fern.png',110,554)
local worker=Image{fromFile='assets/Monkey/Spider Worker/spider_monkey_idle.png'}
assert(worker.width==64 and worker.height==64)
local actors=axi.cel('03 Exact native worker and banana pixels - review only',1)
local positions={{246,335},{432,375},{659,290},{762,496},{356,520}}
for _,p in ipairs(positions) do actors.image:drawImage(worker,Point(p[1]-32,p[2]-56)) end
local bananas=Image{fromFile='assets/Banana/Banana.png'}
local fruit=Image(16,16,ColorMode.RGB);fruit:drawImage(bananas,Point(0,0))
for _,p in ipairs({{280,326},{685,282},{789,486},{388,512}}) do actors.image:drawImage(fruit,Point(p[1],p[2])) end
axi.save(out..'ground-context.aseprite');preview:saveCopyAs(out..'ground-context.png')
local proof=Image{fromFile=out..'ground-context.png'}
local minx,miny,maxx,maxy=64,64,-1,-1
for _,p in ipairs(positions) do for y=0,63 do for x=0,63 do
 local c=worker:getPixel(x,y)
 if app.pixelColor.rgbaA(c)>0 then
  assert(proof:getPixel(p[1]-32+x,p[2]-56+y)==c,'Worker pixels changed')
  minx=math.min(minx,x);miny=math.min(miny,y);maxx=math.max(maxx,x);maxy=math.max(maxy,y)
 end
end end end
local report=assert(io.open(out..'validation.txt','w'))
report:write(string.format('PASS: 64 tiles; 128x64 cells; 4096 opaque pixels each.\nPASS: all ground colors in canonical 31-color palette; binary alpha.\nPASS: source / atlas / individual PNG equality.\nPASS: all 128 compatible edge pairs at 129 samples; all variant edge strips identical.\nPASS: 960x640 assembled scene: every pixel covered exactly once.\nPASS: five unchanged 64x64 worker references, visible bounds %dx%d.\nPASS: sparse authored texture occupies less than 3.5%% of each tile.\nScope: asset exports only; no runtime or economy changes.\n',maxx-minx+1,maxy-miny+1))
report:close()
print('PASS: ground palette, alpha, export equality, 128 compatible edge pairs, variant borders, 614400-pixel tessellation, original worker pixels.')
