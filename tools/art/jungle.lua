-- sprite-axi run tools/art/jungle.lua
-- Five static vegetation sprites, drawn at the Spider Worker's native pixel scale.
local out='assets/Jungle/'
local W,H=320,352
local C={dark='#303843',blue='#405273',plum='#593e47',rose='#7a5859',bark='#734c44',wood='#a57855',tan='#bcad9f',
 shade='#546756',green='#2f4d2f',leaf='#89a477',light='#a4c5af',mint='#cae6d9',olive='#819447',gold='#fdd179',ochre='#de9f47',cream='#fee1b8'}
local palImage=Image{fromFile='docs/references/palette.png'}
local pal=Palette(31);local allowed={}
for x=0,30 do local p=palImage:getPixel(x,0);pal:setColor(x,Color(p));allowed[p]=true end
local s,cel
local assets={}
local function start() s=axi.new(W,H);s:deleteLayer(s.layers[1]);s:setPalette(pal) end
local function layer(n) cel=axi.cel(n,1) end
local function poly(p,c)
 local snapped={};for _,q in ipairs(p) do snapped[#snapped+1]={math.floor(q[1]),math.floor(q[2])} end
 axi.poly(cel,snapped,C[c] or c,{fill=true})
end
local function line(x,y,xx,yy,c,w) axi.line(cel,math.floor(x),math.floor(y),math.floor(xx),math.floor(yy),C[c] or c,{thickness=w or 1}) end
local function rect(x,y,w,h,c) axi.rect(cel,math.floor(x),math.floor(y),w,h,C[c] or c,{fill=true}) end
local function hash(x,y,salt)
 local n=((x*73856093) ~ (y*19349663) ~ (salt*83492791)) & 0xffffffff
 n=((n ~ (n >> 13))*1274126177) & 0xffffffff
 return ((n ~ (n >> 16)) & 0xffffff)/16777216
end
local function save(name)
 local flat=Image(W,H,ColorMode.RGB);flat:drawSprite(s,1)
 local minx,miny,maxx,maxy,count=W,H,-1,-1,0
 local used={}
 for y=0,H-1 do for x=0,W-1 do
  local p=flat:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
  assert(a==0 or a==255,'Partial alpha in '..name)
  if a>0 then
   assert(allowed[p],'Palette drift in '..name);used[p]=true;count=count+1
   minx=math.min(minx,x);miny=math.min(miny,y);maxx=math.max(maxx,x);maxy=math.max(maxy,y)
  end
 end end
 assert(count>0 and minx>0 and miny>0 and maxx<W-1 and maxy<H-1,'Empty or clipped '..name)
 axi.save(out..name..'.aseprite');s:saveCopyAs(out..name..'.png')
 local png=Image{fromFile=out..name..'.png'}
 for y=0,H-1 do for x=0,W-1 do assert(png:getPixel(x,y)==flat:getPixel(x,y),'Export mismatch') end end
 local colors=0;for _ in pairs(used) do colors=colors+1 end
 local a={name=name,image=flat,bounds={minx,miny,maxx,maxy},colors=colors,pixels=count};assets[#assets+1]=a
 print(string.format('%s: bounds (%d,%d)-(%d,%d), %d colors, %d opaque px; palette/alpha/margins/export PASS',name,minx,miny,maxx,maxy,colors,count))
 return a
end
local function ground(w)
 layer('01 Ground shadow')
 -- A circular ground shadow projects to a 2:1 ellipse. No plinth or rim.
 local ry=w/2
 for y=math.ceil(-ry),math.floor(ry) do
  local dx=math.floor(w*math.sqrt(math.max(0,1-(y/ry)^2)))
  rect(160-dx,316+y,dx*2+1,1,'shade')
 end
end
local function crown(lobes,main,shadow,seed)
 local function field(x,y)
  local best=-9
  for _,e in ipairs(lobes) do local d=1-((x-e[1])/e[3])^2-((y-e[2])/e[4])^2;if d>best then best=d end end
  return best
 end
 local mask,bottom={},{}
 for y=8,258,2 do
  mask[y]={}
  for x=8,310,2 do
   local rough=hash(math.floor(x/35),math.floor(y/25),seed)>.5
   local jitter=(hash(math.floor(x/4),math.floor(y/3),seed+1)-.5)*(rough and .13 or .016)
   if field(x,y)+jitter>0 then mask[y][x]=true;bottom[x]=y end
  end
 end
 for y=8,258,2 do
  local run,last=nil,nil
  local function flush(x) if run then rect(run,y,x-run,2,last);run=nil end end
  for x=8,312,2 do
   local color=nil
   if mask[y][x] then
    color=main
    local band=10+math.floor(hash(math.floor(x/8),0,seed+2)*17)
    if y>bottom[x]-band then color=shadow end
    if y>bottom[x]-8 and hash(math.floor(x/17),0,seed+3)>.72 then color='blue' end
    if y>bottom[x]-band-8 and y<bottom[x]-band and hash(math.floor(x/5),math.floor(y/4),seed+4)>.79 then color=shadow end
    if field(x,y)<.07 and hash(math.floor(x/30),math.floor(y/21),seed+5)>.5 and hash(math.floor(x/4),math.floor(y/3),seed+6)>.8 then color=nil end
   end
   if color~=last then flush(x);if color then run=x end;last=color end
  end
  flush(314)
 end
 for x=28,290,11 do
  local xx=x-x%2;local by=bottom[xx]
  if by and hash(x,0,seed+7)>.65 then rect(xx,by-2,2,4+math.floor(hash(x,0,seed+8)*9),shadow) end
 end
 -- Occasional tiny clusters, never all-over dither or concentric highlights.
 for _,p in ipairs({{95,112},{151,80},{214,127},{123,148},{178,130}}) do
  if field(p[1],p[2])>.3 then rect(p[1],p[2],3,2,main=='shade' and 'leaf' or 'light');rect(p[1]+5,p[2]-2,2,2,main=='shade' and 'leaf' or 'light') end
 end
end

start();ground(46)
layer('02 Trunk and buttress roots')
poly({{145,133},{171,127},{179,179},{173,229},{171,278},{178,300},{198,312},{214,316},{201,323},{182,317},{163,311},{151,320},{136,328},{121,326},{140,314},{149,296},{139,281},{127,298},{111,305},{99,302},{124,288},{141,257},{151,210}},'plum')
poly({{151,144},{160,139},{163,199},{158,238},{158,283},{154,302},{142,317},{129,322},{146,307},{151,289},{149,254},{154,212}},'rose')
poly({{165,163},{171,183},{166,234},{165,279},{170,303},{188,313},{204,319},{189,315},{163,305},{158,285},{161,247}},'bark')
poly({{139,278},{147,279},{138,298},{119,303},{134,294}},'bark')
poly({{158,242},{161,258},{159,274},{155,283},{156,262}},'wood')
layer('03 Branches and hanging lianas')
poly({{153,185},{129,165},{115,132},{92,112},{96,104},{124,126},{141,156},{156,160}},'plum')
poly({{164,189},{179,159},{206,145},{222,124},{230,128},{214,154},{186,172},{172,204}},'plum')
poly({{175,174},{184,160},{205,151},{199,159},{184,166}},'rose')
for _,p in ipairs({{73,175,229},{110,187,239},{233,178,226}}) do
 line(p[1],p[2],p[1]-2,p[3],'shade');rect(p[1]-4,p[3]-11,3,5,'leaf')
end
layer('04 Broad unoutlined crown')
crown({{145,105,106,76},{84,84,48,48},{215,104,68,62},{73,143,48,49},{239,153,47,43},{154,61,67,36}},'shade','blue',41)
layer('05 Moss at root contact')
poly({{124,316},{137,312},{145,317},{140,321},{129,323},{123,321}},'leaf')
poly({{189,312},{196,314},{201,313},{207,318},{195,319}},'shade')
save('jungle-broad')

start();ground(34)
layer('02 Leaning trunk and roots')
poly({{178,132},{195,128},{183,181},{159,230},{155,271},{166,303},{184,315},{193,317},{183,322},{165,315},{152,321},{138,326},{125,324},{144,313},{143,290},{136,264},{144,222},{165,179}},'plum')
poly({{177,146},{183,143},{174,181},{151,230},{147,265},{153,291},{157,309},{150,315},{150,293},{143,267},{148,226},{169,181}},'rose')
poly({{169,202},{163,224},{155,245},{154,273},{163,302},{176,315},{166,309},{154,288},{149,264},{154,238}},'bark')
layer('03 Open fork')
poly({{154,225},{142,198},{119,181},{105,155},{112,151},{127,174},{151,189},{165,211}},'plum')
poly({{170,204},{185,187},{209,181},{225,162},{231,168},{216,190},{193,195},{175,222}},'plum')
poly({{143,198},{129,181},{120,177},{127,186},{146,207}},'rose')
line(232,188,230,242,'shade');rect(227,227,4,5,'leaf')
layer('04 Offset open crown')
crown({{158,143,71,56},{111,177,53,39},{225,174,59,45},{203,117,43,45},{155,110,59,34}},'leaf','shade',63)
layer('05 Root moss')
poly({{128,320},{137,316},{146,318},{142,322},{132,324}},'leaf')
save('jungle-leaning')

-- Banana leaf paddles follow curved midribs. Wide blades have sparse torn slots,
-- not palm-like needles. Only a short portion of the midrib is highlighted.
local function leaf(ax,ay,cx,cy,bx,by,width,color,shadow,seed)
 local function point(t)
  local q=1-t
  local x=q*q*ax+2*q*t*cx+t*t*bx;local y=q*q*ay+2*q*t*cy+t*t*by
  local dx=2*q*(cx-ax)+2*t*(bx-cx);local dy=2*q*(cy-ay)+2*t*(by-cy)
  local length=math.sqrt(dx*dx+dy*dy);return x,y,-dy/length,dx/length
 end
 local p={}
 for side=1,-1,-2 do
  for j=0,24 do
   local i=side==1 and j or 24-j;local t=i/24
   local x,y,nx,ny=point(t)
   local w=width*math.sin(math.pi*t)^.7
   if (i==13+seed%3 or i==19-seed%2) and side==(seed%2==0 and 1 or -1) then w=w*.25 end
   p[#p+1]={math.floor(x+side*nx*w),math.floor(y+side*ny*w)}
  end
 end
 poly(p,color)
 -- A narrow underside exposed along one folded portion, not a perimeter stroke.
 local fold={}
 for i=9,22 do local t=i/24;local x,y,nx,ny=point(t);fold[#fold+1]={math.floor(x),math.floor(y)} end
 for i=22,9,-1 do local t=i/24;local x,y,nx,ny=point(t);local w=width*math.sin(math.pi*t)^.7*.62;fold[#fold+1]={math.floor(x-nx*w),math.floor(y-ny*w)} end
 poly(fold,shadow)
 for i=4,10 do local x,y=point(i/24);local xx,yy=point((i+1)/24);line(x,y,xx,yy,color=='shade' and 'leaf' or 'light') end
end
local function bananaPlant()
 start();ground(31)
 layer('02 Green pseudostem and old sheaths')
 poly({{152,190},{167,191},{168,237},{163,278},{170,310},{165,317},{151,317},{146,311},{150,276},{153,235}},'shade')
 poly({{156,197},{164,195},{160,251},{156,281},{161,312},{154,316},{151,309},{154,277},{157,241}},'leaf')
 poly({{157,265},{160,254},{158,285},{163,306},{160,312},{157,298}},'light')
 poly({{148,266},{154,283},{151,303},{148,313},{141,317},{144,309},{147,291}},'bark')
 poly({{167,288},{170,305},{178,313},{182,314},{173,316},{166,308}},'rose')
 layer('03 Back leaf paddles')
 line(160,204,151,185,'shade',3)
 leaf(154,186,116,113,131,98,18,'shade','blue',2)
 line(161,205,175,183,'shade',3)
 leaf(175,184,199,131,249,146,17,'leaf','shade',3)
 line(155,203,133,189,'shade',3)
 leaf(135,190,84,154,49,181,20,'leaf','shade',4)
 layer('04 Lateral torn paddles')
 leaf(157,204,109,173,68,235,22,'olive','shade',6)
 leaf(162,204,220,171,274,229,21,'leaf','shade',5)
 layer('05 Foreground drooping leaves')
 leaf(157,209,114,223,110,279,19,'leaf','shade',7)
 leaf(164,210,195,237,207,286,20,'shade','green',8)
 layer('06 Crown shoot')
 poly({{158,201},{155,173},{160,158},{166,169},{166,191},{163,207}},'leaf')
 poly({{158,177},{159,164},{163,172},{162,195}},'light')
end
local function fruit(x,y,flip)
 local f=flip or 1
 poly({{x,y},{x+f,y+5},{x+4*f,y+8},{x+9*f,y+7},{x+13*f,y+2},{x+11*f,y+10},{x+6*f,y+13},{x+f,y+11},{x-2*f,y+6}},'ochre')
 poly({{x,y},{x+2*f,y+5},{x+5*f,y+7},{x+9*f,y+6},{x+13*f,y+2},{x+10*f,y+9},{x+6*f,y+11},{x+2*f,y+9},{x,y+5}},'gold')
 line(x+2*f,y+4,x+5*f,y+6,'cream',2)
 rect(x-1,y-2,2,3,'bark')
end
bananaPlant()
layer('07 Hanging ripe bunch')
line(165,201,181,211,'shade',4);line(181,211,185,243,'shade',3)
for _,p in ipairs({{173,219,1},{194,222,-1},{170,232,1},{196,236,-1},{176,245,1}}) do fruit(p[1],p[2],p[3]) end
line(184,251,183,271,'shade',2)
poly({{183,269},{189,272},{188,280},{181,289},{178,281},{178,275}},'plum')
poly({{182,272},{185,273},{183,282},{180,284},{180,279}},'rose')
local ripe=save('banana-fruiting')
-- Same plant pixels, with only the fruit layer removed and its cut stalk retained.
local fruitLayer=cel.layer;s:deleteLayer(fruitLayer)
layer('07 Cut fruit stalk')
line(165,201,181,211,'shade',4);line(181,211,182,220,'shade',3)
line(180,219,184,220,'tan',2)
local bare=save('banana-harvested')
local changed=0
for y=0,H-1 do for x=0,W-1 do
 if ripe.image:getPixel(x,y)~=bare.image:getPixel(x,y) then
  changed=changed+1;assert(x>=163 and x<=198 and y>=199 and y<=291,'Harvest changes plant outside fruit/stalk')
 end
end end
assert(changed>0,'Harvest state unchanged')
print('PASS: harvest states differ only in the fruit and cut-stalk region.')

start();ground(37)
layer('02 Low fern fronds')
-- Wide and low: the center is below a worker's knee, even with long outer fronds.
for _,p in ipairs({{151,311,115,277,89,286,7},{154,309,145,273,124,265,7},{159,308,164,275,180,272,7},{162,311,191,284,224,290,8},{154,313,125,292,90,315,9},{160,313,192,302,213,326,9}}) do
 local ax,ay,cx,cy,bx,by,w=table.unpack(p)
 line(ax,ay,cx,cy,'shade',2)
 -- Paired broad pinnules follow each arched spine rather than triangular fans.
 local function pt(t) local q=1-t;return q*q*ax+2*q*t*cx+t*t*bx,q*q*ay+2*q*t*cy+t*t*by end
 for i=2,7 do
  local t=i/8;local x,y=pt(t);local xx,yy=pt(math.min(1,t+.1));local dx,dy=xx-x,yy-y;local len=math.sqrt(dx*dx+dy*dy)
  local nx,ny=-dy/len,dx/len;local size=w*(1-.45*t)
  for _,side in ipairs({-1,1}) do poly({{x-2,y+1},{x+nx*size*side-dx,y+ny*size*side-dy},{x+nx*size*side+dx*.8,y+ny*size*side+dy*.8},{xx,yy}},i%3==0 and 'shade' or 'leaf') end
  line(x,y,xx,yy,'shade')
 end
end
layer('03 Young curled frond')
line(157,312,160,292,'shade',2)
line(160,292,165,287,'leaf',2);line(165,287,170,289,'leaf',2);line(170,289,168,294,'leaf',2)
rect(157,311,6,3,'olive')
save('jungle-fern')

-- Fixed-cell atlas and placement metadata.
local atlas=axi.new(W*3,H*2);atlas:deleteLayer(atlas.layers[1]);atlas:setPalette(pal)
local ac=axi.cel('Vegetation sprites',1)
for i,a in ipairs(assets) do ac.image:drawImage(a.image,Point(((i-1)%3)*W,math.floor((i-1)/3)*H)) end
axi.save(out..'jungle-atlas.png')
local json=assert(io.open(out..'jungle-atlas.json','w'))
json:write('{\n  "image": "jungle-atlas.png", "cellWidth": 320, "cellHeight": 352, "columns": 3,\n  "anchor": {"x": 160, "y": 316}, "projection": "2:1 ground axes",\n  "sprites": [\n')
for i,a in ipairs(assets) do json:write(string.format('    {"name":"%s","x":%d,"y":%d,"w":320,"h":352,"bounds":[%d,%d,%d,%d]}%s\n',a.name,((i-1)%3)*W,math.floor((i-1)/3)*H,a.bounds[1],a.bounds[2],a.bounds[3],a.bounds[4],i<#assets and ',' or '')) end
json:write('  ]\n}\n');json:close()
-- Native-scale review sheet; original worker pixels are copied without rescaling.
local preview=axi.new(960,800);preview:deleteLayer(preview.layers[1]);preview:setPalette(pal)
local bg=axi.cel('Quiet backdrop',1);axi.rect(bg,0,0,960,800,C.light,{fill=true})
local props=axi.cel('Vegetation at native resolution',1)
local worker=Image{fromFile='docs/references/spider-worker.png'};assert(worker.width==64 and worker.height==64)
local workers=axi.cel('Exact Spider Worker references',1)
for i,a in ipairs(assets) do
 local x=((i-1)%3)*320;local y=32+math.floor((i-1)/3)*384
 props.image:drawImage(a.image,Point(x,y));workers.image:drawImage(worker,Point(x+216,y+259))
end
workers.image:drawImage(worker,Point(768,32+384+259))
-- Minimal 5x7 pixel lettering, doubled for legibility in an asset review sheet.
local font={A={'01110','10001','10001','11111','10001','10001','10001'},B={'11110','10001','10001','11110','10001','10001','11110'},C={'01111','10000','10000','10000','10000','10000','01111'},D={'11110','10001','10001','10001','10001','10001','11110'},E={'11111','10000','10000','11110','10000','10000','11111'},F={'11111','10000','10000','11110','10000','10000','10000'},G={'01111','10000','10000','10111','10001','10001','01111'},H={'10001','10001','10001','11111','10001','10001','10001'},I={'11111','00100','00100','00100','00100','00100','11111'},J={'00111','00010','00010','00010','10010','10010','01100'},K={'10001','10010','10100','11000','10100','10010','10001'},L={'10000','10000','10000','10000','10000','10000','11111'},M={'10001','11011','10101','10101','10001','10001','10001'},N={'10001','11001','10101','10011','10001','10001','10001'},O={'01110','10001','10001','10001','10001','10001','01110'},P={'11110','10001','10001','11110','10000','10000','10000'},R={'11110','10001','10001','11110','10100','10010','10001'},S={'01111','10000','10000','01110','00001','00001','11110'},T={'11111','00100','00100','00100','00100','00100','00100'},U={'10001','10001','10001','10001','10001','10001','01110'},V={'10001','10001','10001','10001','10001','01010','00100'},W={'10001','10001','10001','10101','10101','11011','10001'},X={'10001','10001','01010','00100','01010','10001','10001'},Y={'10001','10001','01010','00100','00100','00100','00100'},['-']={'00000','00000','00000','11111','00000','00000','00000'},['1']={'00100','01100','00100','00100','00100','00100','01110'},[':']={'00000','00100','00100','00000','00100','00100','00000'}}
local text=axi.cel('Labels - preview only',1)
local function label(str,x,y,scale)
 for ch in str:gmatch('.') do local glyph=font[ch];if glyph then for row,bits in ipairs(glyph) do for col=1,5 do if bits:sub(col,col)=='1' then axi.rect(text,x+(col-1)*scale,y+(row-1)*scale,scale,scale,C.shade,{fill=true}) end end end end;x=x+6*scale end
end
label('JUNGLE AND BANANA TREES',24,10,2)
local labels={'JUNGLE - BROAD','JUNGLE - LEANING','BANANA - FRUITING','BANANA - HARVESTED','JUNGLE - FERN'}
for i,str in ipairs(labels) do label(str,((i-1)%3)*320+24,32+math.floor((i-1)/3)*384+352,2) end
label('SPIDER WORKER',664,768,2);label('NATIVE PIXELS - 1:1',654,435,2)
axi.save(out..'jungle-scale-preview.aseprite');preview:saveCopyAs(out..'jungle-scale-preview.png')
local proof=Image{fromFile=out..'jungle-scale-preview.png'}
for i=1,5 do
 local ox=((i-1)%3)*320+216;local oy=32+math.floor((i-1)/3)*384+259
 for y=0,63 do for x=0,63 do local p=worker:getPixel(x,y);if app.pixelColor.rgbaA(p)>0 then assert(proof:getPixel(ox+x,oy+y)==p,'Worker reference modified') end end end
end
print('PASS: native-scale worker references unchanged; five layered masters, transparent PNGs, atlas and context preview.')
