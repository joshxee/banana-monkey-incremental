-- Original pixel art constructed in Aseprite through sprite-axi.
-- Rebuild: sprite-axi run tools/art/town-center.lua
-- Art direction and reference observations: assets/TownCenter/README.md.
local s=axi.new(672,704)
local out='assets/TownCenter/'
local C={ink='#14233a',blue='#405273',slate='#6c81a1',mist='#96a9c1',
 dark='#303843',plum='#593e47',rose='#7a5859',bark='#734c44',wood='#a57855',
 tan='#bcad9f',sand='#d4c692',rust='#b55945',coral='#eb9661',
 gold='#fdd179',ochre='#de9f47',cream='#fee1b8',
 leaf='#89a477',light='#a4c5af',mint='#cae6d9',shade='#546756',green='#2f4d2f',olive='#819447'}
local palImage=Image{fromFile='docs/references/palette.png'}
local pal=Palette(31)
for x=0,30 do pal:setColor(x,Color(palImage:getPixel(x,0))) end
s:setPalette(pal)
local cel
local function layer(n) cel=axi.cel(n,1) end
local function poly(p,c) axi.poly(cel,p,C[c] or c,{fill=true}) end
local function line(x,y,xx,yy,c,w) axi.line(cel,math.floor(x),math.floor(y),math.floor(xx),math.floor(yy),C[c] or c,{thickness=w or 1}) end
local function rect(x,y,w,h,c) axi.rect(cel,math.floor(x),math.floor(y),w,h,C[c] or c,{fill=true}) end
-- Exact 2:1 ground axes. Native pixels; never enlarge the monkey.
local function P(u,v,z) return {math.floor(330+u-v),math.floor(440+(u+v)/2-z)} end
local function face(a,c) local p={};for _,v in ipairs(a) do p[#p+1]=P(v[1],v[2],v[3]) end;poly(p,c) end
local function edge(a,b,c,w) local p=P(table.unpack(a));local q=P(table.unpack(b));line(p[1],p[2],q[1],q[2],c,w) end
local function box(u,v,w,d,z,h,top,front,side)
 face({{u,v,z+h},{u+w,v,z+h},{u+w,v+d,z+h},{u,v+d,z+h}},top)
 face({{u,v+d,z+h},{u+w,v+d,z+h},{u+w,v+d,z},{u,v+d,z}},front)
 face({{u+w,v,z+h},{u+w,v+d,z+h},{u+w,v+d,z},{u+w,v,z}},side)
end
local function hash(x,y,salt)
 local n=((x*73856093) ~ (y*19349663) ~ ((salt or 0)*83492791)) & 0xffffffff
 n=((n ~ (n >> 13))*1274126177) & 0xffffffff
 return ((n ~ (n >> 16)) & 0xffffff)/16777216
end

layer('01 Quiet ground and cast shade')
-- An irregular clearing, not a solid dark isometric plinth.
poly({{100,432},{173,398},{306,389},{442,417},{552,475},{533,498},{566,517},{523,543},{496,557},{443,553},{427,572},{343,580},{302,568},{250,575},{207,553},{162,551},{147,530},{87,515},{72,486}},'light')
poly({{128,436},{236,406},{338,405},{453,444},{499,473},{477,492},{438,494},{422,515},{360,523},{322,512},{278,521},{250,503},{194,500},{177,479},{146,477}},'shade')
-- Quiet warm landing and collection apron let the existing dark monkey pop.
poly({{95,481},{169,475},{226,504},{192,531},{148,548},{90,534},{63,519}},'tan')
poly({{310,520},{379,480},{500,520},{553,548},{510,577},{448,591},{371,570},{327,557}},'tan')
for _,p in ipairs({{124,549},{104,559},{83,570}}) do
 poly({{p[1]-12,p[2]},{p[1],p[2]-6},{p[1]+17,p[2]+2},{p[1]+5,p[2]+8}},'tan')
end

layer('02 Living trunk and root buttresses')
poly({{302,237},{349,227},{355,328},{346,403},{360,443},{391,466},{414,476},{396,482},{367,471},{341,456},{329,471},{318,481},{290,494},{272,489},{301,472},{312,447},{294,431},{278,448},{251,455},{239,449},{266,437},{288,410},{302,358}},'plum')
poly({{307,244},{326,239},{328,319},{319,388},{324,432},{318,458},{302,476},{285,485},{312,465},{317,439},{303,420},{300,393},{312,331}},'rose')
poly({{331,264},{341,253},{340,350},{332,399},{339,439},{358,458},{384,471},{365,467},{331,449},{326,418},{328,378}},'bark')
poly({{306,420},{304,440},{293,453},{278,457},{286,451},{297,436}},'wood')
poly({{349,445},{362,453},{373,466},{366,464},{354,455}},'rose')
-- Root moss is grouped into a few deliberate patches.
poly({{276,480},{288,478},{298,482},{290,488},{278,489},{272,486}},'leaf')
poly({{383,471},{393,470},{406,476},{397,478},{393,475}},'shade')

layer('03 Braces and suspended deck')
-- Posts only where the structure needs them; no dark wireframe perimeter.
for _,p in ipairs({{-121,98},{123,100},{127,-75}}) do box(p[1],p[2],7,7,0,96,'wood','rose','plum') end
edge({124,102,12},{76,102,91},'plum',7)
edge({-120,99,18},{-83,99,90},'rose',6)
box(-130,-108,266,218,88,8,'tan','rose','plum')
-- Sunlight is a broad area, never a bright stroke around the whole object.
face({{-130,78,96},{112,78,96},{136,110,96},{-130,110,96}},'sand')
face({{112,-98,96},{136,-108,96},{136,90,96},{112,78,96}},'tan')
for _,u in ipairs({-111,-72,54,108}) do edge({u,94,96},{u,108,96},'wood') end
edge({-126,110,94},{-91,110,94},'tan',2)
edge({34,110,91},{76,110,91},'bark',2)

layer('04 Sunbleached house and shadowed side')
-- Four-room exterior around the tree: large color planes, sparse timber marks.
box(-112,-90,224,168,96,68,'rose','tan','rose')
face({{-112,78,151},{112,78,151},{112,78,164},{-112,78,164}},'plum')
face({{112,-90,146},{112,78,146},{112,78,164},{112,-90,164}},'plum')
-- Ragged cast shadow from an overhanging roof, not an outline.
for _,a in ipairs({{-98,13,148},{-67,9,145},{-20,22,149},{45,15,146},{86,25,144}}) do
 face({{a[1],78,a[3]},{a[1]+a[2],78,a[3]},{a[1]+a[2],78,153},{a[1],78,153}},'plum')
end
-- One structural seam and small repairs carry more character than a plank grid.
edge({23,79,98},{23,79,142},'wood',3)
face({{-108,79,103},{-88,79,103},{-88,79,110},{-108,79,110}},'wood')
edge({-99,79,106},{-89,79,106},'tan')
edge({63,79,119},{69,79,119},'wood')
edge({77,79,116},{80,79,116},'wood')
-- Tall open doorway; pale deck beneath it isolates the dark character.
face({{-58,79,96},{-25,79,96},{-25,79,150},{-58,79,150}},'plum')
face({{-56,79,96},{-32,79,96},{-32,79,146},{-56,79,146}},'dark')
face({{-27,80,98},{-18,80,103},{-18,80,146},{-27,80,150}},'wood')
edge({-56,80,149},{-32,80,149},'rose',2)
-- Small cool windows; yellow is reserved for bananas.
local function windowFront(u,w)
 face({{u,79,116},{u+w,79,116},{u+w,79,143},{u,79,143}},'blue')
 face({{u+3,79,118},{u+w-3,79,118},{u+w-3,79,133},{u+3,79,133}},'slate')
 edge({u+w/2,80,119},{u+w/2,80,141},'rose',2)
 face({{u-3,79,114},{u+w+4,79,114},{u+w+4,85,117},{u-3,85,117}},'rose')
end
windowFront(-100,22);windowFront(54,29)
for _,v in ipairs({-69,15}) do
 face({{113,v,116},{113,v+27,116},{113,v+27,143},{113,v,143}},'dark')
 face({{113,v+3,119},{113,v+24,119},{113,v+24,133},{113,v+3,133}},'blue')
 edge({114,v+14,120},{114,v+14,140},'plum',2)
 edge({114,v-2,115},{114,v+30,115},'bark',3)
end

layer('05 Weathered irregular roof')
-- Huge dried fronds, draped over timber ribs, replace a mechanical hipped roof.
-- The ribs keep the isometric footprint, but the material sags and tears.
poly({{108,280},{175,242},{281,212},{340,203},{407,225},{462,252},{524,280},{556,298},{542,311},{518,316},{446,350},{395,384},{363,397},{343,386},{301,372},{248,342},{196,318},{148,298}},'plum')
poly({{108,277},{160,245},{212,228},{276,212},{330,207},{359,216},{323,232},{291,254},{258,282},{229,312},{213,310},{202,303},{182,303},{169,298},{149,298},{135,290},{119,287}},'rust')
poly({{219,312},{237,281},{268,248},{301,227},{338,213},{374,220},{390,235},{374,253},{356,277},{339,310},{321,345},{305,355},{294,348},{282,347},{271,338},{257,337},{246,330},{236,329}},'rust')
poly({{288,348},{309,304},{338,259},{367,233},{392,231},{415,248},{415,274},{406,305},{393,336},{388,362},{371,390},{361,385},{356,379},{345,380},{334,373},{322,374},{311,365},{301,361}},'rust')
poly({{374,238},{391,227},{413,231},{449,248},{486,271},{522,285},{552,294},{559,289},{556,301},{544,308},{525,310},{508,319},{491,319},{470,333},{449,341},{427,357},{409,365},{389,381},{374,390},{389,356},{398,324},{404,288},{400,261}},'rose')
-- Short veins and frayed slits; they never surround a whole roof plane.
poly({{310,351},{320,324},{340,289},{354,271},{342,291},{326,325},{320,346},{316,351}},'rose')
poly({{225,305},{230,290},{243,275},{235,291},{233,305}},'plum')
poly({{352,380},{357,368},{362,354},{360,373},{359,385}},'plum')
poly({{448,340},{455,326},{465,317},{458,328},{454,337}},'plum')
line(263,284,271,275,'rose',2);line(285,271,289,267,'rose')
line(420,294,427,289,'bark',2)
rect(287,319,4,2,'rose');rect(292,322,2,2,'rose')
-- One stitched repair adds human-scale use without a repeated texture grid.
poly({{245,298},{257,286},{275,295},{264,310}},'rose')
for _,p in ipairs({{249,298},{254,293},{266,301}}) do line(p[1],p[2],p[1]+3,p[2]+2,'tan') end
poly({{318,275},{338,263},{354,270},{359,283},{350,291},{336,290},{325,295},{316,286}},'shade')
poly({{320,278},{333,268},{341,272},{335,280},{328,281},{326,287},{319,283}},'leaf')

layer('06 Forked trunk through the roof')
poly({{306,275},{313,237},{307,202},{291,182},{269,174},{254,153},{263,146},{281,162},{303,163},{315,178},{318,146},{307,116},{318,112},{330,138},{335,170},{358,153},{377,145},{394,119},{404,124},{391,153},{367,171},{342,192},{341,232},{350,269},{330,281}},'plum')
poly({{310,270},{319,235},{314,201},{303,183},{310,184},{324,200},{324,236},{321,264},{329,277}},'rose')
poly({{325,144},{322,163},{326,190},{323,207},{330,224},{329,251},{336,272},{343,268},{335,231},{337,191},{350,178},{346,175},{330,184}},'bark')
poly({{322,236},{325,253},{322,267},{327,273},{323,273},{318,266}},'wood')
poly({{287,168},{302,169},{314,183},{311,190},{302,178}},'rose')
line(323,253,323,260,'tan')

layer('07 Flat crown with broken leaf clusters')
-- One connected foliage field. Shadow belongs to its underside; no halo outlines,
-- concentric highlight bands, or separately shaded polygon lobes.
local lobes={{274,118,131,80},{181,147,94,79},{139,188,60,55},{232,176,83,49},{429,172,105,81},{485,210,71,49},{372,150,54,46},{327,86,74,49}}
local function strength(x,y)
 local best=-9
 for _,e in ipairs(lobes) do local d=1-((x-e[1])/e[3])^2-((y-e[2])/e[4])^2;if d>best then best=d end end
 return best
end
local mask,base={},{}
for y=26,282,2 do
 mask[y]={}
 for x=64,586,2 do
  local d=strength(x,y)
  local rough=hash(math.floor(x/42),math.floor(y/32),31)>.48
  local jitter=(hash(math.floor(x/4),math.floor(y/3),4)-.5)*(rough and .13 or .018)
  if d+jitter>0 then mask[y][x]=true;base[x]=y end
 end
end
for y=26,282,2 do
 local start,lastColor=nil,nil
 local function flush(x)
  if start then rect(start,y,x-start,2,lastColor);start=nil end
 end
 for x=64,588,2 do
  local color=nil
  if mask[y][x] then
   local bottom=base[x]
   local band=11+math.floor(hash(math.floor(x/9),0,8)*18)
   color='leaf'
   if y>bottom-band then color='shade' end
   if y>bottom-10 and hash(math.floor(x/19),0,5)>.53 then color='blue' end
   -- Broken shadow islands mostly along the lower quarter.
   if y>bottom-band-13 and y<bottom-band and hash(math.floor(x/5),math.floor(y/4),7)>.7 then color='shade' end
   if strength(x,y)<.07 and hash(math.floor(x/36),math.floor(y/24),31)>.55 and hash(math.floor(x/4),math.floor(y/3),12)>.72 then color=nil end
  end
  if color~=lastColor then flush(x);if color then start=x end;lastColor=color end
 end
 flush(590)
end
-- Sparse, low-contrast clusters. The center remains a large calm color field.
for _,p in ipairs({{181,129},{222,103},{256,87},{371,91},{452,131},{482,164},{351,133},{123,203},{402,208},{260,193}}) do
 rect(p[1],p[2],4,2,'light');rect(p[1]+5,p[2]-3,2,3,'light')
end
-- Inward notches and leaf tufts concentrate complexity at the lower silhouette.
for x=94,552,7 do
 local bx=x-x%2;local by=base[bx]
 if by and hash(x,0,14)>.35 then
  local len=3+math.floor(hash(x,0,3)*12)
  rect(bx,by-4,2,len, 'shade')
  if x%3==0 then rect(bx+3,by-1,3,5,'shade') end
 end
end
for _,p in ipairs({{122,241,276},{181,251,287},{482,245,287},{521,237,269}}) do
 line(p[1],p[2],p[1],p[3],'shade')
 rect(p[1]-2,p[3]-9,3,5,'leaf');rect(p[1]+1,p[3]-19,3,4,'leaf')
end
-- A few sizeable torn clumps alternate with quiet runs of silhouette.
poly({{135,225},{146,222},{155,229},{155,236},{163,236},{162,243},{153,242},{153,248},{143,247},{143,242},{133,242},{133,235},{127,235},{129,229}},'shade')
poly({{133,225},{143,222},{149,227},{147,233},{139,232},{139,238},{132,235}},'leaf')
poly({{466,238},{476,232},{485,235},{485,242},{492,244},{491,250},{484,249},{484,258},{477,256},{477,249},{467,249}},'shade')
poly({{467,234},{477,231},{483,234},{481,240},{475,239},{472,244},{466,241}},'leaf')

layer('08 Open stair and sparse balustrade')
-- Twelve risers reach ground exactly. Avoid outlining tread edges.
for i=12,1,-1 do
 local v=110+(i-1)*7;local z=96-i*8
 box(-69,v,43,7,z,8,'sand','wood','rose')
 if i==3 or i==8 then edge({-66,v+3,z+8},{-57,v+3,z+8},'tan') end
end
-- A few substantial handrails replace the fence-like grid.
for _,a in ipairs({{-127,-81},{-13,37},{71,129}}) do
 for _,u in ipairs({a[1],a[2]}) do box(u,107,3,3,96,23,'tan','wood','rose') end
 edge({a[1],109,119},{a[2],109,119},'tan',4)
end
for _,u in ipairs({-71,-24}) do
 edge({u,114,118},{u,194,20},'rose',4)
 edge({u,114,119},{u,194,21},'tan',2)
end

layer('09 Civic cloth and gathered belongings')
-- A long asymmetrical cloth is one civic accent, not a set of luminous windows.
face({{113,-1,151},{113,27,151},{113,27,100},{113,19,107},{113,12,104},{113,-1,110}},'rust')
face({{114,0,151},{114,5,151},{114,5,112},{114,0,113}},'coral')
-- Pale banana emblem uses tan so real fruit remains the brightest subject.
local b=P(115,12,138)
poly({{b[1]-4,b[2]},{b[1]-4,b[2]+9},{b[1]+1,b[2]+17},{b[1]+8,b[2]+20},{b[1]+12,b[2]+16},{b[1]+5,b[2]+15},{b[1]+1,b[2]+10},{b[1]-1,b[2]}},'tan')
-- Small rolled mat and earthen jars, quietly colored.
box(90,83,18,14,96,7,'shade','leaf','shade')
local j=P(-104,96,96)
poly({{j[1]-7,j[2]-15},{j[1]+5,j[2]-15},{j[1]+9,j[2]-7},{j[1]+6,j[2]+1},{j[1]-5,j[2]+3},{j[1]-10,j[2]-5}},'rose')
rect(j[1]-5,j[2]-17,8,3,'plum')

layer('10 Collection bins and golden bananas')
local function banana(x,y,flip)
 local f=flip or 1
 poly({{x,y},{x+2*f,y+6},{x+6*f,y+9},{x+12*f,y+8},{x+17*f,y+2},{x+14*f,y+9},{x+9*f,y+13},{x+3*f,y+12},{x-2*f,y+7},{x-2*f,y+2}},'ochre')
 poly({{x,y},{x+2*f,y+5},{x+6*f,y+8},{x+11*f,y+7},{x+17*f,y+2},{x+13*f,y+9},{x+8*f,y+11},{x+3*f,y+10},{x,y+6}},'gold')
 line(x+2*f,y+5,x+5*f,y+7,'cream',2)
 line(x+6*f,y+8,x+10*f,y+7,'cream')
 rect(x-1,y-2,2,3,'bark')
end
local function bin(u,v,w,d,seed)
 box(u,v,w,d,0,27,'blue','slate','blue')
 face({{u+3,v+3,27},{u+w-3,v+3,27},{u+w-3,v+d-3,27},{u+3,v+d-3,27}},'dark')
 -- Loose hand-placed bunches overlap irregularly; no identical tiled yellow dots.
 for _,a in ipairs({{8,7,1},{25,7,-1},{w-10,13,1},{11,d-9,1},{w-14,d-8,-1},{22,16,1}}) do
  local p=P(u+a[1],v+a[2],30+(a[1]+seed)%5);banana(p[1]-5,p[2]-4,a[3])
 end
 edge({u,v+d,28},{u+w,v+d,28},'mist',3)
 edge({u+w,v,28},{u+w,v+d,28},'slate',3)
 edge({u+5,v+d,10},{u+w-6,v+d,10},'blue')
 edge({u+w-5,v+d,2},{u+w-5,v+d,24},'mist',2)
 -- Inset grip and a single rivet, rather than outlined slats.
 local p=P(u+w/2,v+d+1,18);rect(p[1]-5,p[2],10,3,'dark')
end
bin(90,54,46,33,1)
bin(147,71,43,34,2)
bin(131,116,49,37,3)
-- A fallen bunch links the storage to the landing.
banana(403,585,1)

layer('11 Small growth and fallen leaves')
for _,p in ipairs({{192,492},{255,533},{490,489},{540,539},{298,540},{152,447}}) do
 line(p[1],p[2],p[1]-3,p[2]-9,'shade',2)
 rect(p[1]-7,p[2]-7,5,3,'leaf');rect(p[1],p[2]-11,5,3,'leaf')
end
for _,p in ipairs({{219,523},{229,526},{490,573},{260,432},{549,505}}) do
 rect(p[1],p[2],3,2,'olive')
end

-- Pixel-level integrity, not a substitute for visual review.
s:deleteLayer(s.layers[1])
local flat=Image(s.width,s.height,ColorMode.RGB);flat:drawSprite(s,1)
local allowed,used={},{}
for x=0,30 do allowed[palImage:getPixel(x,0)]=true end
local count,minx,miny,maxx,maxy=0,s.width,s.height,-1,-1
for y=0,s.height-1 do for x=0,s.width-1 do
 local px=flat:getPixel(x,y);local a=app.pixelColor.rgbaA(px)
 assert(a==0 or a==255,'Partial alpha')
 if a==255 then
  assert(allowed[px],'Color outside reference palette');used[px]=true;count=count+1
  minx=math.min(minx,x);miny=math.min(miny,y);maxx=math.max(maxx,x);maxy=math.max(maxy,y)
 end
end end
assert(minx>0 and miny>0 and maxx<s.width-1 and maxy<s.height-1,'Clipping')
local colors=0;for _ in pairs(used) do colors=colors+1 end
print(string.format('Verified %d opaque pixels; %d exact palette colors; bounds (%d,%d)-(%d,%d); binary alpha.',count,colors,minx,miny,maxx,maxy))
axi.save(out..'town-center.aseprite');s:saveCopyAs(out..'town-center.png')
local exported=Image{fromFile=out..'town-center.png'}
for y=0,s.height-1 do for x=0,s.width-1 do assert(flat:getPixel(x,y)==exported:getPixel(x,y),'Export differs') end end

-- Context proof: unchanged source monkey at 1:1 on quiet surfaces.
local mi=Image{fromFile='docs/references/spider-worker.png'}
assert(mi.width==64 and mi.height==64)
local preview=axi.new(800,736)
local bg=axi.cel('Muted field',1);axi.rect(bg,0,0,800,736,C.light,{fill=true})
local art=axi.cel('Town center',1);art.image:drawImage(flat,Point(32,34))
local workers=axi.cel('Spider Workers - original pixels',1)
local positions={{124,505},{446,572},{271,395}}
for _,p in ipairs(positions) do workers.image:drawImage(mi,Point(p[1],p[2])) end
-- The third worker stands on the deck: contrast is tested against the building too.
axi.save(out..'town-center-scale-preview.aseprite');preview:saveCopyAs(out..'town-center-scale-preview.png')
local proof=Image{fromFile=out..'town-center-scale-preview.png'}
for y=0,63 do for x=0,63 do local px=mi:getPixel(x,y);if app.pixelColor.rgbaA(px)>0 then
 for _,p in ipairs(positions) do assert(proof:getPixel(x+p[1],y+p[2])==px,'Reference monkey changed') end
end end end
print('PASS: exported PNG equals layered master; all three workers retain exact source pixels at 1:1.')
preview:close()
