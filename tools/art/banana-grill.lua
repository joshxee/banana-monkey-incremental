-- Rebuild the standalone grill: sprite-axi run tools/art/banana-grill.lua
local kitchen='assets/BananaGrill/'
app.fs.makeAllDirectories(kitchen)
local pi=Image{fromFile='docs/references/palette.png'}
local palette,allowed=Palette(31),{}
for x=0,30 do local p=pi:getPixel(x,0);palette:setColor(x,Color(p));allowed[p]=true end
local C={ink='#3d3333',shadow='#593e47',furDark='#636663',fur='#87857c',mane='#bcad9f',
 maneLight='#d5d6db',pink='#b55945',skin='#f2b888',skinShade='#eb9661',
 cream='#f1f6f0',cloth='#bbc3d0',fold='#96a9c1',metal='#303843',iron='#405273',
 steel='#6c81a1',stone='#87857c',stoneLight='#bcad9f',stoneDark='#636663',
 ember='#b55945',hot='#eb9661',gold='#fdd179',goldShade='#de9f47',bananaLight='#fee1b8',
 ground='#546756',wood='#734c44'}
local cel
local function poly(pts,c) axi.poly(cel,pts,C[c] or c,{fill=true}) end
local function line(pts,c,w) for i=1,#pts-1 do axi.line(cel,pts[i][1],pts[i][2],pts[i+1][1],pts[i+1][2],C[c] or c,{thickness=w or 1}) end end
local function rect(x,y,w,h,c) axi.rect(cel,x,y,w,h,C[c] or c,{fill=true}) end
local function layer(name,frame) cel=axi.cel(name,frame or 1) end
local function imageOf(s,f) local im=Image(s.width,s.height,ColorMode.RGB);im:drawSprite(s,f or 1);return im end
local function equal(a,b)
 assert(a.width==b.width and a.height==b.height,'Dimensions differ')
 for y=0,a.height-1 do for x=0,a.width-1 do assert(a:getPixel(x,y)==b:getPixel(x,y),'Pixel mismatch') end end
end
local checked=0
local function validate(im)
 for y=0,im.height-1 do for x=0,im.width-1 do local p=im:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
 assert(a==0 or a==255,'Partial alpha')
 if a>0 then assert(allowed[p],'Palette drift');assert(x>0 and x<im.width-1 and y>0 and y<im.height-1,'Clipped export') end
 end end;checked=checked+1
end
local function savePng(im,path) validate(im);im:saveAs(path);equal(im,Image{fromFile=path}) end
local grill=axi.new(128,112);grill:setPalette(palette);grill:deleteLayer(grill.layers[1])
layer('01 Stone feet')
poly({{26,76},{37,76},{40,95},{33,100},{23,95}},'stoneDark')
poly({{88,73},{100,74},{105,93},{96,99},{88,94}},'stoneDark')
poly({{27,80},{33,82},{34,95},{26,93}},'stone')
poly({{91,79},{99,77},{101,92},{96,95}},'stone')
layer('02 Hearth')
-- Irregular salvaged masonry beneath an open, old-fashioned iron grate.
poly({{16,59},{65,37},{112,60},{110,80},{65,103},{17,80}},'stoneDark')
poly({{18,60},{65,82},{65,101},{18,78}},'stone')
poly({{20,61},{63,81},{63,86},{19,66}},'stoneLight')
poly({{65,82},{111,60},{109,79},{65,101}},'furDark')
poly({{69,84},{82,78},{82,86},{69,93}},'metal')
poly({{86,76},{104,67},{103,76},{87,84}},'metal')
line({{25,76},{36,81},{36,88}},'stoneDark');line({{46,84},{46,92},{59,98}},'stoneDark')
line({{85,87},{85,92},{98,86}},'stone')
layer('03 Charcoal and embers')
poly({{17,58},{65,35},{112,58},{65,82}},'metal')
poly({{26,57},{64,39},{103,58},{65,76}},'ink')
for _,p in ipairs({{44,52},{60,46},{79,52},{91,58},{65,67},{47,62},{74,60}}) do
 poly({{p[1]-3,p[2]},{p[1],p[2]-2},{p[1]+5,p[2]},{p[1]+2,p[2]+3}},'ember')
 line({{p[1],p[2]+1},{p[1]+3,p[2]+1}},'hot')
end
rect(75,86,3,1,'ember');rect(94,76,3,1,'hot')
layer('04 Iron grate')
line({{15,55},{65,30},{114,55},{65,80},{15,55}},'iron',3)
line({{16,54},{65,30},{113,54}},'steel')
for i=1,7 do local dx=i*6;line({{18+dx,54-math.floor(dx/2)},{65+dx,77-math.floor(dx/2)}},'iron',2) end
-- Offset handles and a small historic forge repair.
line({{15,53},{9,50},{9,56},{16,60}},'metal',2)
line({{112,54},{119,51},{119,57},{112,61}},'metal',2)
rect(88,41,5,2,'stoneLight')
layer('05 Grilling bananas')
local function banana(x,y)
 poly({{x,y},{x+3,y+3},{x+9,y+5},{x+15,y+4},{x+19,y},{x+18,y+5},{x+14,y+8},{x+8,y+9},{x+3,y+6}},'goldShade')
 line({{x+2,y+1},{x+5,y+4},{x+10,y+6},{x+15,y+5},{x+18,y+2}},'gold',2)
 line({{x+4,y+3},{x+8,y+5},{x+12,y+5}},'bananaLight')
 rect(x,y-1,2,2,'wood');rect(x+18,y-1,2,2,'wood')
 line({{x+7,y+5},{x+6,y+7}},'wood');line({{x+12,y+5},{x+11,y+7}},'wood')
end
banana(52,38);banana(32,50);banana(72,50);banana(53,61)
local grillImage=imageOf(grill);savePng(grillImage,kitchen..'banana-grill.png');axi.save(kitchen..'banana-grill.aseprite')
local savedGrill=app.open(kitchen..'banana-grill.aseprite');equal(imageOf(savedGrill),grillImage);savedGrill:close()

print('PASS: grill palette, alpha, margins, PNG round trip and saved master pixels agree.')

