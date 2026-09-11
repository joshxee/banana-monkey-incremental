-- Review-only looping route with actual Spider Worker and town-center boxes.
-- Run from repo root: sprite-axi run tools/art/squirrel-monkey-preview.lua
local out='assets/Monkey/Squirrel Unpacker/'
local file=assert(io.open(out..'squirrel-monkey.json','r'));local meta=json.decode(file:read('*a'));file:close()
local clips,sheets={},{}
for _,c in ipairs(meta.clips) do clips[c.name]=c;if not sheets[c.sheet] then sheets[c.sheet]=Image{fromFile=out..c.sheet} end end
local workerEmpty=Image{fromFile='assets/Monkey/Spider Worker/spider_monkey_walk_8dir.png'}
local workerFull=Image{fromFile='assets/Monkey/Spider Worker/spider_monkey_carry_walk_8dir.png'}
local town=Image{fromFile='assets/TownCenter/town-center.png'}
local fruit=Image{fromFile=out..'banana-transfer.png'}
local fruitFlipped=Image(24,24,ColorMode.RGB)
for y=0,23 do for x=0,23 do fruitFlipped:drawPixel(23-x,y,fruit:getPixel(x,y)) end end
local function frame(name,i)
 local c=clips[name];local r=c.frames[i];local im=Image(64,64,ColorMode.RGB)
 im:drawImage(sheets[c.sheet],Point(-r.x,-r.y));return im
end
local function round(n) return math.floor(n+.5) end
local function drawFruit(im,x,y,flip) im:drawImage(flip and fruitFlipped or fruit,Point(round(x)-(flip and 13 or 10),round(y)-10)) end
-- Exact front-box wall projection from town-center.lua: bin(131,116,49,37).
-- Copy the approved pixels, including its rim, over fruit sinking into the box.
local function frontWall(x,y)
 if x<28 or x>114 then return false end
 local top=x<=77 and 114+(x-28)*.5 or 138-(x-77)*.5
 return y>=math.floor(top)-1 and y<=math.floor(top)+29
end
local function occlude(im)
 for y=112,168 do for x=28,114 do if frontWall(x,y) then im:drawPixel(x,y,town:getPixel(x+280,y+440)) end end end
end
local function worker(full)
 local im=Image(64,64,ColorMode.RGB);im:drawImage(full and workerFull or workerEmpty,Point(0,-6*64));return im
end
local function scene(t)
 local im=Image(440,200,ColorMode.RGB);im:clear(app.pixelColor.rgba(164,197,175,255))
 im:drawImage(town,Point(-280,-440))
 local x,y,name,phase=132,156,'idle_E',1
 if t<600 then phase=1+math.floor(t/300)%4
 elseif t<1500 then x=132+(282-132)*(t-600)/900;name='dart_E';phase=1+math.floor((t-600)/50)%8
 elseif t<1920 then x=282;name='take_E';phase=1+math.floor((t-1500)/70)
 elseif t<2100 then x=282;name='take_E';phase=6
 elseif t<3100 then x=282-(282-132)*(t-2100)/1000;name='carry_W';phase=1+math.floor((t-2100)/50)%8
 elseif t<3520 then name='drop_NW';phase=1+math.floor((t-3100)/70)
 else name='idle_NW';phase=1+math.floor((t-3520)/300)%4 end
 im:drawImage(worker(t<1500),Point(330-32,156-56))
 im:drawImage(frame(name,phase),Point(round(x)-32,y-56))
 -- During receipt the donor is empty and exactly one external banana moves to the hands.
 if t>=1500 and t<1710 then
  local u=(t-1500)/210;local h=clips.take_E.frames[4].bananaSocket
  drawFruit(im,315+(282-32+h.x-315)*u,133+(156-56+h.y-133)*u)
 end
 -- Release at drop frame 4; a short descending arc crosses the open box rim.
 if t>=3310 and t<3600 then
  local u=(t-3310)/290;local h=clips.drop_NW.frames[3].bananaSocket
  local sx,sy=132-32+h.x,156-56+h.y
  local x,y
  if u<.65 then local v=u/.65;x=sx+(80-sx)*v;y=sy+(118-sy)*v-12*math.sin(v*math.pi)
  else x=80;y=118+27*(u-.65)/.35 end
  drawFruit(im,x,y,clips.drop_NW.bananaFlipX);occlude(im)
 end
 return im
end
local preview=Sprite(880,400,ColorMode.RGB)
for i=1,90 do
 if i>1 then preview:newEmptyFrame() end
 preview.frames[i].duration=.05
 local native=scene((i-1)*50);local big=Image(880,400,ColorMode.RGB)
 for y=0,399 do for x=0,879 do big:drawPixel(x,y,native:getPixel(math.floor(x/2),math.floor(y/2))) end end
 preview:newCel(preview.layers[1],i,big,Point(0,0))
 if i==56 then big:saveAs(out..'squirrel-monkey-delivery.png') end
end
preview:saveCopyAs(out..'squirrel-monkey-delivery.gif');preview:close()
local timeline=Image(440*3,200*2,ColorMode.RGB)
for i,t in ipairs({1500,1700,1750,3300,3500,3550}) do timeline:drawImage(scene(t),Point(((i-1)%3)*440,math.floor((i-1)/3)*200)) end
timeline:saveAs(out..'squirrel-monkey-transfer-study.png')
-- The final released sprite is entirely behind the physical front wall before removal.
for y=0,23 do for x=0,23 do if app.pixelColor.rgbaA(fruitFlipped:getPixel(x,y))>0 then
 assert(frontWall(80-13+x,145-10+y),'Fruit not fully occluded before removal')
end end end
-- Complete motion grid: every direction, matching phase for empty/loaded cycles.
local grid=Sprite(1024,256,ColorMode.RGB)
for i=1,8 do
 if i>1 then grid:newEmptyFrame() end;grid.frames[i].duration=.05
 local native=Image(512,128,ColorMode.RGB);native:clear(app.pixelColor.rgba(164,197,175,255))
 for col,d in ipairs(meta.directions) do for row,state in ipairs({'dart','carry'}) do native:drawImage(frame(state..'_'..d,i),Point((col-1)*64,(row-1)*64)) end end
 local big=Image(1024,256,ColorMode.RGB)
 for y=0,255 do for x=0,1023 do big:drawPixel(x,y,native:getPixel(math.floor(x/2),math.floor(y/2))) end end
 grid:newCel(grid.layers[1],i,big,Point(0,0))
end
grid:saveCopyAs(out..'squirrel-monkey-darts.gif');grid:close()
print('Exported 4.5 s delivery demonstration and all-direction motion grid.')
