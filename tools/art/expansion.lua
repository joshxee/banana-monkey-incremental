-- Run one asset at a time: $env:JUNGLE_ASSET='fan-palm'; sprite-axi run tools/art/expansion.lua
local name=assert(os.getenv('JUNGLE_ASSET'),'Set JUNGLE_ASSET')
local out='assets/Expansion/'
app.fs.makeAllDirectories(out)
local W,H=320,352
local s=axi.new(W,H);s:deleteLayer(s.layers[1])
local palette=Image{fromFile='docs/references/palette.png'}
local pal=Palette(31);local allowed={}
for x=0,30 do local p=palette:getPixel(x,0);pal:setColor(x,Color(p));allowed[p]=true end
s:setPalette(pal)
local C={dark='#303843',blue='#405273',plum='#593e47',rose='#7a5859',bark='#734c44',wood='#a57855',tan='#bcad9f',shade='#546756',green='#2f4d2f',leaf='#89a477',light='#a4c5af',mint='#cae6d9',olive='#819447',gold='#fdd179',ochre='#de9f47',cream='#fee1b8'}
local cel
local function layer(n) cel=axi.cel(n,1) end
local function poly(p,c) local q={};for _,v in ipairs(p) do q[#q+1]={math.floor(v[1]),math.floor(v[2])} end;axi.poly(cel,q,C[c] or c,{fill=true}) end
local function rect(x,y,w,h,c) axi.rect(cel,math.floor(x),math.floor(y),w,h,C[c] or c,{fill=true}) end
local function line(x,y,a,b,c,w) axi.line(cel,math.floor(x),math.floor(y),math.floor(a),math.floor(b),C[c] or c,{thickness=w or 1}) end
local function oval(x,y,rx,ry,c)
 for dy=-ry,ry do local dx=math.floor(rx*math.sqrt(math.max(0,1-(dy/ry)^2)));rect(x-dx,y+dy,dx*2+1,1,c) end
end
local function blade(x,y,a,b,width,c)
 local dx,dy=a-x,b-y;local d=math.sqrt(dx*dx+dy*dy)
 poly({{x,y},{x+dx*.43-dy/d*width,y+dy*.43+dx/d*width},{a,b},{x+dx*.55+dy/d*width*.6,y+dy*.55-dx/d*width*.6}},c)
end
local function crown(lobes,main,shadow)
 local rows,bottom={},{}
 for y=20,218,2 do rows[y]={};for x=24,292,2 do
  local field=-10
  for _,v in ipairs(lobes) do field=math.max(field,1-((x-v[1])/v[3])^2-((y-v[2])/v[4])^2) end
  local jitter=((math.floor(x/6)*17+math.floor(y/5)*29)%19-9)*.009
  if field+jitter>0 then rows[y][x]=true;bottom[x]=y end
 end end
 for y,row in pairs(rows) do for x in pairs(row) do
  local band=9+(math.floor(x/8)*13)%15
  rect(x,y,2,2,y>bottom[x]-band and shadow or main)
 end end
end
if name=='fan-palm' then
 layer('01 Curving trunk');poly({{147,318},{154,277},{169,227},{172,148},{183,143},{187,222},{171,279},{166,318}},'bark')
 poly({{151,316},{159,279},{177,223},{177,153},{181,153},{182,225},{164,279},{160,317}},'wood')
 layer('02 Fan leaves')
 for _,fan in ipairs({{175,159,88,103},{181,151,220,77},{179,158,261,163},{173,154,148,69},{175,160,101,213}}) do
 local x,y,a,b=table.unpack(fan);local angle=math.atan(b-y,a-x)
 for j=-3,3 do local t=angle+j*.16;local len=86-math.abs(j)*5;blade(x,y,x+math.cos(t)*len,y+math.sin(t)*len,10,j<0 and 'shade' or 'leaf') end
 end
 layer('03 Folded emerging spear');blade(179,151,186,54,8,'olive')
elseif name=='buttress-tree' then
 layer('01 Broad sculpted roots');poly({{147,135},{174,127},{173,244},{184,282},{233,319},{203,317},{172,292},{166,321},{147,327},{143,291},{105,319},{83,318},{132,273}},'plum')
 poly({{155,146},{164,142},{161,259},{164,316},{154,320},{149,275}},'rose');poly({{169,228},{177,280},{219,314},{201,310},{163,279}},'bark')
 layer('02 Tiered spreading crown')
 crown({{143,67,66,33},{95,104,62,31},{202,102,81,33},{157,141,94,36}},'shade','blue')
 layer('03 Broken hanging clusters');for _,v in ipairs({{78,128},{112,166},{194,173},{251,119}}) do rect(v[1],v[2],5,13,'shade');rect(v[1]+7,v[2]-4,7,9,'shade') end
elseif name=='bamboo' then
 layer('01 Jointed stems')
 for _,v in ipairs({{133,91,145},{153,64,159},{178,103,170},{198,133,182},{113,151,137}}) do
 line(v[1],v[2],v[3],316,'shade',7);line(v[1]-2,v[2],v[3]-2,311,'olive',3)
 for y=v[2]+22,299,28 do local x=v[1]+(v[3]-v[1])*(y-v[2])/(316-v[2]);line(x-3,y,x+3,y,'light',2) end
 end
 layer('02 Airy leaf sprays')
 for _,v in ipairs({{137,125,-1},{157,85,1},{176,148,1},{145,206,-1},{185,221,1},{156,264,-1}}) do
 local x,y,d=table.unpack(v);line(x,y,x+d*62,y-25,'shade',2)
 for j=1,4 do local a=x+d*j*13;local b=y-j*5;blade(a,b,a+d*24,b-26,5,'leaf');blade(a,b,a+d*30,b+7,5,'shade') end
 end
elseif name=='vine-tree' then
 layer('01 Bent fork');poly({{159,320},{145,277},{149,211},{124,169},{107,117},{116,113},{139,160},{164,185},{191,135},{203,129},{184,181},{168,220},{162,273},{176,316}},'plum')
 poly({{158,313},{152,274},{156,218},{152,196},{163,209},{159,277},{166,315}},'rose')
 layer('02 Drooping crown')
 crown({{98,125,50,53},{158,99,62,49},{218,145,52,60}},'leaf','shade')
 layer('03 Curtains of vines')
 for _,v in ipairs({{67,155,242},{88,174,265},{114,166,228},{203,186,251},{233,181,273},{253,165,237}}) do
 line(v[1],v[2],v[1]-4,v[3],'shade',2)
 for y=v[2]+12,v[3]-5,18 do blade(v[1]-2,y,v[1]-12,y+12,4,'leaf') end
 end
elseif name=='fallen-log' then
 layer('01 Broken log');poly({{76,287},{191,260},{218,271},{234,291},{210,305},{96,322},{76,311}},'plum')
 poly({{80,286},{191,260},{216,269},{198,285},{91,307}},'bark')
 layer('02 Exposed split');poly({{198,267},{217,273},{228,292},{209,300},{196,284}},'wood');poly({{205,275},{213,278},{218,291},{208,291}},'plum')
 layer('03 Moss and fern growth');poly({{89,283},{113,276},{131,280},{152,271},{178,269},{189,276},{158,283},{141,282},{123,291},{99,292}},'shade')
 for _,v in ipairs({{122,281,94,251},{123,281,144,244},{156,276,181,244}}) do blade(v[1],v[2],v[3],v[4],10,'leaf') end
elseif name=='deep-jungle-floor' then
 s:close();W,H=128,64;s=axi.new(W,H);s:deleteLayer(s.layers[1]);s:setPalette(pal)
 layer('01 Quiet deep green diamond')
 for y=0,63 do for x=0,127 do local u=(x+.5-64)/128+(y+.5)/64;local v=-(x+.5-64)/128+(y+.5)/64;if u>=0 and v>=0 and u<1 and v<1 then rect(x,y,1,1,'shade') end end end
 layer('02 Sparse leaf litter');poly({{49,27},{59,24},{66,25},{57,28}},'green');poly({{73,38},{78,36},{86,38},{79,40}},'green')
elseif name=='chef-kitchen' then
 layer('01 Open cooking floor');poly({{18,298},{151,231},{303,307},{229,344},{110,344}},'shade')
 layer('02 Side preparation counter');poly({{20,281},{43,269},{79,287},{56,299}},'tan');poly({{20,281},{56,299},{56,310},{20,292}},'wood');poly({{56,299},{79,287},{79,298},{56,310}},'bark')
 rect(26,294,5,25,'bark');rect(65,303,5,20,'bark')
 layer('03 Serving counter');poly({{273,292},{291,283},{308,292},{290,301}},'tan');poly({{273,292},{290,301},{290,316},{273,308}},'wood');poly({{290,301},{308,292},{308,307},{290,316}},'bark')
 layer('04 Food and utensils');oval(44,283,10,4,'wood');poly({{38,280},{46,282},{52,281},{48,285},{42,285}},'cream');line(54,285,68,291,'blue',2);oval(290,291,6,3,'cream')
else
 layer('01 Foundation');poly({{44,263},{164,203},{276,259},{156,319}},'shade')
 layer('02 Timber posts')
 local posts={{66,266},{164,217},{255,262}}
 for _,v in ipairs(posts) do rect(v[1],v[2]-104,7,104,'bark');rect(v[1],v[2]-100,2,96,'wood') end
 if name=='research-hut' then
 layer('03 Recessed laboratory');poly({{70,183},{163,137},{252,181},{252,260},{162,305},{70,259}},'tan');poly({{162,229},{252,184},{252,260},{162,305}},'rose')
 poly({{179,235},{211,219},{211,274},{179,290}},'dark');poly({{91,204},{134,226},{134,247},{91,226}},'blue')
 layer('04 Asymmetric leaf roof');poly({{48,175},{157,113},{273,170},{258,190},{220,207},{184,220},{160,234},{122,212},{87,198},{63,190}},'shade');poly({{48,175},{157,113},{171,168},{160,234},{122,212},{87,198}},'leaf')
 layer('05 Research telescope and chart');line(203,145,225,107,'wood',5);poly({{203,106},{220,94},{244,113},{229,125}},'blue');poly({{220,94},{226,91},{249,109},{244,113}},'light')
 poly({{91,242},{121,257},{111,271},{81,256}},'cream');line(93,251,109,258,'blue',2)
 layer('06 Telescope lens and roof growth');oval(240,112,8,7,'dark');oval(241,111,5,4,'light')
 poly({{49,176},{70,181},{76,193},{68,192},{62,187},{57,188}},'leaf')
 poly({{97,199},{115,207},{120,217},{113,215},{108,210},{104,213}},'leaf')
 poly({{179,220},{194,212},{190,226},{184,237},{180,232}},'shade')
 poly({{226,198},{241,193},{238,206},{230,218},{231,204}},'shade')
 poly({{109,157},{129,160},{132,169},{124,182},{106,174}},'shade');line(111,163,116,165,'leaf',2);line(114,171,120,174,'leaf',2)
 layer('07 Crooked porch bench');poly({{76,261},{115,281},{105,287},{68,269}},'wood');rect(76,269,4,12,'bark');rect(102,281,4,9,'bark')
 elseif name=='distribution-center' then
 layer('03 Open storage bays');poly({{71,170},{164,124},{252,168},{252,263},{164,306},{164,212},{71,258}},'plum')
 layer('04 Sagging canopy');poly({{48,153},{156,98},{274,155},{261,174},{218,192},{165,214},{115,188},{69,170}},'rose');poly({{48,153},{156,98},{170,151},{165,214},{115,188},{69,170}},'bark')
 layer('05 Sorting bins')
 for _,v in ipairs({{89,257},{132,278},{208,257}}) do local x,y=table.unpack(v);poly({{x-18,y-9},{x,y-18},{x+22,y-7},{x+4,y+2}},'blue');poly({{x-18,y-9},{x+4,y+2},{x+4,y+25},{x-18,y+14}},'light');poly({{x+4,y+2},{x+22,y-7},{x+22,y+16},{x+4,y+25}},'blue');line(x-12,y-2,x-2,y+3,'shade',2) end
 layer('06 Small banana shipment');for j=0,2 do poly({{84+j*6,245},{86+j*6,252},{91+j*6,255},{95+j*6,250},{91+j*6,260},{86+j*6,258}},'gold') end
 layer('07 Repaired fabric and open bay');poly({{95,151},{113,148},{130,162},{119,178},{99,167}},'rose');line(101,155,105,158,'wood',2);line(111,163,115,166,'wood',2)
 poly({{67,171},{91,181},{94,191},{88,188},{85,183},{79,185},{73,179}},'bark')
 poly({{220,190},{239,181},{237,189},{229,192},{225,198}},'rose')
 line(167,220,222,193,'wood',3);line(181,222,181,233,'wood',2)
 poly({{174,228},{191,222},{191,237},{174,244}},'tan');line(178,235,187,230,'blue',2)
 layer('08 Worn loading ramp');poly({{158,305},{193,288},{212,300},{177,318}},'wood');line(174,302,191,310,'bark',2)
 else error('Unknown asset '..name) end
end
local flat=Image(W,H);flat:drawSprite(s,1)
local count=0
for y=0,H-1 do for x=0,W-1 do local p=flat:getPixel(x,y);local a=app.pixelColor.rgbaA(p);assert(a==0 or a==255,'partial alpha');if a>0 then assert(allowed[p],'palette drift');count=count+1;if name~='deep-jungle-floor' then assert(x>0 and y>0 and x<W-1 and y<H-1,'clipped') end end end end
assert(count>0)
axi.save(out..name..'.aseprite');s:saveCopyAs(out..name..'.png')
if name=='research-hut' or name=='distribution-center' or name=='chef-kitchen' then
 local floor=s.layers[1]
 assert(floor.name=='01 Foundation' or floor.name=='01 Open cooking floor')
 floor.isVisible=false
 s:saveCopyAs(out..name..'-structure.png')
 for _,l in ipairs(s.layers) do l.isVisible=l==floor end
 s:saveCopyAs(out..name..'-ground.png')
 for _,l in ipairs(s.layers) do l.isVisible=true end
 local joined=Image{fromFile=out..name..'-ground.png'}
 joined:drawImage(Image{fromFile=out..name..'-structure.png'})
 for y=0,H-1 do for x=0,W-1 do assert(joined:getPixel(x,y)==flat:getPixel(x,y),'split export mismatch') end end
end
local png=Image{fromFile=out..name..'.png'}
for y=0,H-1 do for x=0,W-1 do assert(png:getPixel(x,y)==flat:getPixel(x,y),'export mismatch') end end
local preview=Image(W+80,H);local bg=Color{r=164,g=197,b=175,a=255}.rgbaPixel;preview:clear(bg);preview:drawImage(flat)
if name~='deep-jungle-floor' then preview:drawImage(Image{fromFile='docs/references/spider-worker.png'},Point(W-12,262)) end
preview:saveAs(out..name..'-preview.png')
print(name..': palette, binary alpha, margins and master/export agreement PASS; '..count..' pixels')
