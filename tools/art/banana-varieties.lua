-- Run from repository root: sprite-axi run tools/art/banana-varieties.lua
-- Editable pixel art; authored Bezier profiles are rasterized to integer clusters.
-- References: docs/references/bananas/{single,bunch,half-peeled}.png
local out='assets/Banana/Varieties/'
app.fs.makeAllDirectories(out)
local W,H=96,96
local extra={'#b87828','#f3be32','#ffdc52','#fff08a','#2666a0','#398ed8','#80d6f3','#c0f3ff','#94374b','#db5361','#ff9382','#ffc1a1','#d5ba7e','#fff0c2','#fff9e8'}
local pi=Image{fromFile='docs/references/palette.png'}
local pal=Palette(31+#extra);local allowed={}
for x=0,30 do local p=pi:getPixel(x,0);pal:setColor(x,Color(p));allowed[p]=true end
for i,c in ipairs(extra) do local co=Color{r=tonumber(c:sub(2,3),16),g=tonumber(c:sub(4,5),16),b=tonumber(c:sub(6,7),16)};pal:setColor(30+i,co);allowed[co.rgbaPixel]=true end
local paletteImage=Image(31+#extra,1,ColorMode.RGB)
for x=0,30+#extra do paletteImage:drawPixel(x,0,pal:getColor(x).rgbaPixel) end
paletteImage:saveAs(out..'banana-palette.png')
local gpl=assert(io.open(out..'banana-palette.gpl','w'));gpl:write('GIMP Palette\nName: Banana fruit extension\nColumns: 8\n# Canonical palette followed by 15 fruit colors\n')
for x=0,30+#extra do local c=pal:getColor(x);gpl:write(string.format('%d %d %d color-%02d\n',c.red,c.green,c.blue,x)) end;gpl:close()
local variants={
 {id='yellow',label='ORIGINAL',ramp={'#b87828','#f3be32','#ffdc52','#fff08a'},scale=1},
 {id='blue',label='BLUE',ramp={'#2666a0','#398ed8','#80d6f3','#c0f3ff'},scale=1},
 {id='red',label='RED',ramp={'#94374b','#db5361','#ff9382','#ffc1a1'},scale=1},
 {id='huge',label='HUGE',ramp={'#b87828','#f3be32','#ffdc52','#fff08a'},scale=1.45},
 {id='shiny',label='SHINY',ramp={'#de9f47','#ffdc52','#fff08a','#fff9e8'},scale=1}
}
local flesh={'#d5ba7e','#fff0c2','#fff9e8','#fff9e8'}
local s,cel,frame,v
local function round(x) return math.floor(x+.5) end
local function layer(name) cel=axi.cel(name,frame) end
local function poly(points,c)
 local q={};for _,p in ipairs(points) do q[#q+1]={round(p[1]),round(p[2])} end
 axi.poly(cel,q,c,{fill=true})
end
local function line(x,y,xx,yy,c) axi.line(cel,round(x),round(y),round(xx),round(yy),c) end
local function rect(x,y,w,h,c) axi.rect(cel,round(x),round(y),w,h,c,{fill=true}) end
local function point(x,y) return {48+(x-48)*v.scale,79+(y-79)*v.scale} end
local function path(points) local p={};for i,q in ipairs(points) do p[i]=point(q[1],q[2]) end;return p end
local function bez(p,t)
 local a=1-t
 return a*a*a*p[1][1]+3*a*a*t*p[2][1]+3*a*t*t*p[3][1]+t*t*t*p[4][1],a*a*a*p[1][2]+3*a*a*t*p[2][2]+3*a*t*t*p[3][2]+t*t*t*p[4][2]
end
local function tangent(p,t)
 local a=1-t
 local dx=3*a*a*(p[2][1]-p[1][1])+6*a*t*(p[3][1]-p[2][1])+3*t*t*(p[4][1]-p[3][1])
 local dy=3*a*a*(p[2][2]-p[1][2])+6*a*t*(p[3][2]-p[2][2])+3*t*t*(p[4][2]-p[3][2])
 local d=math.sqrt(dx*dx+dy*dy);return -dy/d,dx/d
end
-- A narrow neck, full rounded belly and blunt blossom end. Width is unrelated
-- to the highlight bands: the silhouette remains smooth and asymmetrical.
local function width(t,r) return v.scale*(2+r*.65*(math.sin(math.pi*t)^.55)) end
local function band(p,r,lo,hi,a,b,color)
 local q={};local ts={a}
 -- Fixed parameter samples preserve unchanged skin pixels as the cut advances.
 for i=1,95 do local t=i/96;if t>a and t<b then ts[#ts+1]=t end end;ts[#ts+1]=b
 for _,t in ipairs(ts) do local x,y=bez(p,t);local nx,ny=tangent(p,t);local w=width(t,r);q[#q+1]={x+nx*w*lo,y+ny*w*lo} end
 -- Round the physical ends instead of closing the silhouette with a sharp wedge.
 -- Cut boundaries stay flat so the remaining skin meets the exposed flesh.
 if b==1 then
  local x,y=bez(p,b);local nx,ny=tangent(p,b);local w=width(b,r);local mid=w*(lo+hi)/2;local radius=w*(hi-lo)/2
  for j=1,7 do local angle=math.pi*j/8;local across=mid-radius*math.cos(angle);local along=radius*math.sin(angle)
   q[#q+1]={x+nx*across+ny*along,y+ny*across-nx*along}
  end
 end
 for i=#ts,1,-1 do local t=ts[i];local x,y=bez(p,t);local nx,ny=tangent(p,t);local w=width(t,r);q[#q+1]={x+nx*w*hi,y+ny*w*hi} end
 if a==0 then
  local x,y=bez(p,a);local nx,ny=tangent(p,a);local w=width(a,r);local mid=w*(lo+hi)/2;local radius=w*(hi-lo)/2
  for j=1,7 do local angle=math.pi*j/8;local across=mid+radius*math.cos(angle);local along=radius*math.sin(angle)
   q[#q+1]={x+nx*across-ny*along,y+ny*across+nx*along}
  end
 end
 poly(q,color)
end
local function fruit(p,r,ramp,a,b,name)
 layer(name..' volume');band(p,r,-1,1,a,b,ramp[1])
 layer(name..' face');band(p,r,-.57,1,a,b,ramp[2])
 local aa=math.max(a,.12);local bb=math.min(b,.83)
 if bb>aa then band(p,r,.18,.72,aa,bb,ramp[3]) end
end
local bodyPoints={{42,39},{57,45},{62,63},{44,77}}
local function scar(p)
 local x,y=bez(p,1);layer('07 Blossom scar');rect(x-1,y,2,2,'#734c44');rect(x-1,y,2,1,'#a57855')
end
local function stemAt(x,y,dx,dy)
 poly({{x-2,y+1},{x-2+dx,y-5+dy},{x+2+dx,y-5+dy},{x+2,y+1}},'#546756')
 poly({{x,y+1},{x+dx,y-4+dy},{x+2+dx,y-5+dy},{x+2,y+1}},'#819447')
 line(x-2+dx,y-5+dy,x+2+dx,y-5+dy,'#734c44')
 line(x-1+dx,y-5+dy,x+1+dx,y-5+dy,'#bcad9f')
end
local function glint(x,y,size)
 line(x-size,y,x+size,y,'#fff9e8');line(x,y-size,x,y+size,'#fff9e8')
 rect(x,y,1,1,'#f1f6f0')
end
local function flap(p,r,ramp,name)
 fruit(p,r,ramp,0,1,name)
 -- Narrow pale inner surface turns towards the light as the peel folds.
 layer(name..' inner lip');band(p,r,.48,.95,.04,.94,'#fff0c2')
end
local function single(progress)
 local p=path(bodyPoints)
 local cut=.53*progress
 if progress>0 then
  local hx,hy=bez(p,cut);local nx,ny=tangent(p,cut);local rw=width(cut,5.1)
  local k=progress*v.scale
  -- Back strip unfolds behind the exposed fruit, then the two side strips.
  local right={{hx-nx*rw*.65,hy-ny*rw*.65},{hx+11*k,hy-8*k},{hx+22*k,hy+10*k},{hx+15*k,hy+15*k}}
  flap(right,2.1,{v.ramp[1],v.ramp[2],v.ramp[3],v.ramp[4]},'01 Rear peel')
 end
 fruit(p,4.65,flesh,0,math.min(1,cut+.04),'02 Exposed flesh')
 fruit(p,5.1,v.ramp,cut,1,'03 Closed skin')
 if progress>0 then
  local hx,hy=bez(p,cut);local nx,ny=tangent(p,cut);local rw=width(cut,5.1);local k=progress*v.scale
  local left={{hx+nx*rw*.7,hy+ny*rw*.7},{hx-9*k,hy-8*k},{hx-22*k,hy-4*k},{hx-26*k,hy+3*k}}
  flap(left,2.05,{v.ramp[1],v.ramp[2],v.ramp[3],v.ramp[4]},'04 Left peel')
  local front={{hx,hy+2*k},{hx+10*k,hy+5*k},{hx+7*k,hy+14*k},{hx-3*k,hy+15*k}}
  flap(front,3.2,{v.ramp[1],v.ramp[2],v.ramp[3],v.ramp[4]},'05 Front peel')
  -- The cut stalk stays attached to the end of the opening left strip.
  layer('06 Cut stalk');local ex,ey=bez(left,1);line(ex,ey,ex-2*k,ey+2*k,'#546756')
 else
  layer('06 Cut stalk');local x,y=bez(p,0);stemAt(x,y,-1,0)
 end
 scar(p)
end
local bunchPaths={
 {{59,42},{56,55},{38,55},{25,51}},
 {{61,43},{63,62},{42,67},{27,60}},
 {{62,44},{68,67},{50,77},{32,69}},
 {{63,45},{74,66},{62,81},{42,77}}
}
local function bunch()
 for i,points in ipairs(bunchPaths) do
  local p=path(points);fruit(p,2.35+i*.3,v.ramp,0,1,string.format('%02d Bunch finger',i))
  layer(string.format('%02d Finger blossom scar',i));local x,y=bez(p,1);line(x-1,y-1,x-1,y+1,'#734c44')
 end
 layer('06 Shared crown');local x,y=table.unpack(point(61,42));stemAt(x,y,0,-2)

end
local function start()
 s=axi.new(W,H);s:deleteLayer(s.layers[1]);s:setPalette(pal);frame=1
end
local function flat(sprite,f) local img=Image(W,H,ColorMode.RGB);img:drawSprite(sprite,f);return img end
local function equal(a,b)
 if a.width~=b.width or a.height~=b.height then return false end
 for y=0,a.height-1 do for x=0,a.width-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end;return true
end
local checked=0
local function validate(img,name)
 local bounds={W,H,-1,-1}
 for y=0,H-1 do for x=0,W-1 do
  local p=img:getPixel(x,y);local a=app.pixelColor.rgbaA(p);assert(a==0 or a==255,'Partial alpha '..name)
  if a>0 then
   assert(allowed[p],'Palette drift '..name);assert(x>1 and x<W-2 and y>1 and y<H-2,'Clipped '..name)
   bounds[1]=math.min(bounds[1],x);bounds[2]=math.min(bounds[2],y);bounds[3]=math.max(bounds[3],x);bounds[4]=math.max(bounds[4],y)
  end
 end end
 assert(bounds[3]>=bounds[1],'Empty '..name);checked=checked+1;return bounds
end
local function saveStatic(name)
 local img=flat(s,1);local bounds=validate(img,name)
 axi.save(out..name..'.aseprite');s:saveCopyAs(out..name..'.png')
 assert(equal(img,Image{fromFile=out..name..'.png'}),'PNG mismatch')
 local disk=app.open(out..name..'.aseprite');assert(equal(img,flat(disk,1)),'Saved master mismatch');disk:close();app.activeSprite=s
 return {image=img,bounds=bounds,name=name}
end
local durations={180,80,80,80,80,80,80,80,80,80,80,320}
local all={};local entries={}
for _,variant in ipairs(variants) do
 v=variant;local data={id=v.id,label=v.label,states={}}
 start();single(0);data.states[1]=saveStatic('banana-'..v.id)
 start();bunch();data.states[2]=saveStatic('banana-'..v.id..'-bunch')
 start();single(1);data.states[3]=saveStatic('banana-'..v.id..'-half-peeled')
 start();local frames={}
 -- Predeclare the full stack so late-appearing peel layers never cover the flesh.
 for _,name in ipairs({'01 Rear peel volume','01 Rear peel face','01 Rear peel inner lip','02 Exposed flesh volume','02 Exposed flesh face','03 Closed skin volume','03 Closed skin face','04 Left peel volume','04 Left peel face','04 Left peel inner lip','05 Front peel volume','05 Front peel face','05 Front peel inner lip','06 Cut stalk','07 Blossom scar','08 Specular glints'}) do layer(name) end
 for i,ms in ipairs(durations) do
  if i>1 then s:newEmptyFrame() end;frame=i;s.frames[i].duration=ms/1000
  local t=(i-1)/(#durations-1);single(t*t*(3-2*t));frames[i]=flat(s,i);validate(frames[i],v.id..' peel '..i)
 end
 assert(equal(frames[1],data.states[1].image),'Closed/peeling seam')
 assert(equal(frames[#frames],data.states[3].image),'Half-peeled/peeling seam')
 -- Lower closed fruit and blossom are fixed throughout the action.
 for i=2,#frames do for y=round(79-2*v.scale),H-1 do for x=0,W-1 do assert(frames[i]:getPixel(x,y)==frames[1]:getPixel(x,y),'Ground contact moved') end end end
 axi.tag('peel',1,#frames,{direction='forward'})
 local name='banana-'..v.id..'-peel';axi.save(out..name..'.aseprite')
 local disk=app.open(out..name..'.aseprite');assert(#disk.frames==#frames,'Frame count')
 assert(disk.tags[1].name=='peel' and disk.tags[1].fromFrame.frameNumber==1 and disk.tags[1].toFrame.frameNumber==#frames,'Tag range')
 for i,img in ipairs(frames) do assert(equal(img,flat(disk,i)),'Animation master mismatch');assert(math.abs(disk.frames[i].duration*1000-durations[i])<.01,'Timing mismatch') end
 disk:close();app.activeSprite=s
 local sheet=Image(W*#frames,H,ColorMode.RGB);for i,img in ipairs(frames) do sheet:drawImage(img,Point((i-1)*W,0)) end
 sheet:saveAs(out..name..'-sheet.png');assert(equal(sheet,Image{fromFile=out..name..'-sheet.png'}),'Sheet mismatch')
 data.frames=frames;all[#all+1]=data
 print(v.id..': PASS 3 static states + 12 peel frames, source/PNG agreement, exact action endpoints, fixed ground contact')
end
-- Three rows: closed singles, bunches, half-peeled; five columns: varieties.
local atlas=Image(W*5,H*3,ColorMode.RGB)
for col,data in ipairs(all) do for row,state in ipairs(data.states) do atlas:drawImage(state.image,Point((col-1)*W,(row-1)*H)) end end
atlas:saveAs(out..'banana-varieties-atlas.png');assert(equal(atlas,Image{fromFile=out..'banana-varieties-atlas.png'}),'Atlas mismatch')
local json=assert(io.open(out..'banana-varieties.json','w'))
json:write('{\n "schemaVersion":2,"image":"banana-varieties-atlas.png","cellWidth":96,"cellHeight":96,"columns":5,"rows":3,\n "anchor":{"x":48,"y":80},"sampling":"nearest","palette":"banana-palette.png","boundsFormat":"inclusive minX,minY,maxX,maxY",\n "sprites":[\n')
local forms={'single','bunch','half-peeled'}
for col,data in ipairs(all) do for row,state in ipairs(data.states) do
 local b=state.bounds
 json:write(string.format('  {"name":"%s","variety":"%s","form":"%s","x":%d,"y":%d,"w":96,"h":96,"bounds":[%d,%d,%d,%d]}%s\n',state.name,data.id,forms[row],(col-1)*W,(row-1)*H,b[1],b[2],b[3],b[4],col==#all and row==3 and '' or ','))
end end
json:write(' ],\n "animations":{\n')
for vi,data in ipairs(all) do
 json:write(string.format('  "%s":{"source":"banana-%s-peel.aseprite","image":"banana-%s-peel-sheet.png","loop":false,"holdLastFrame":true,"durationMs":1300,"asepriteFrames":[1,12],"frames":[',data.id,data.id,data.id))
 for i,ms in ipairs(durations) do json:write(string.format('%s{"x":%d,"y":0,"w":96,"h":96,"durationMs":%d}',i>1 and ',' or '',(i-1)*W,ms)) end
 json:write(']}'..(vi<#all and ',' or '')..'\n')
end
json:write(' }\n}\n');json:close()
local jf=assert(io.open(out..'banana-varieties.json','r'));local metadata=jf:read('*a');jf:close()
local js=assert(io.open(out..'banana-varieties-data.js','w'));js:write('window.BANANA_ASSETS = '..metadata..';\n');js:close()
-- Native comparison and action review. Same worker pixels, no rescaling.
local workerPath='assets/Monkey/Spider Worker/spider_monkey_idle.png'
if not app.fs.isFile(workerPath) then workerPath='docs/references/spider-worker.png' end
local worker=Image{fromFile=workerPath};assert(worker.width==64 and worker.height==64)
local font={A={14,17,17,31,17,17,17},B={30,17,17,30,17,17,30},C={14,17,16,16,16,17,14},D={30,17,17,17,17,17,30},E={31,16,16,30,16,16,31},F={31,16,16,30,16,16,16},G={14,17,16,23,17,17,14},H={17,17,17,31,17,17,17},I={31,4,4,4,4,4,31},K={17,18,20,24,20,18,17},L={16,16,16,16,16,16,31},N={17,25,25,21,19,19,17},O={14,17,17,17,17,17,14},P={30,17,17,30,16,16,16},R={30,17,17,30,20,18,17},S={15,16,16,14,1,1,30},U={17,17,17,17,17,17,14},W={17,17,17,21,21,27,17},Y={17,17,10,4,4,4,4}}
local function label(img,str,x,y,color)
 color=color or app.pixelColor.rgba(48,56,67,255)
 for ch in str:gmatch('.') do local glyph=font[ch];if glyph then for yy,bits in ipairs(glyph) do for xx=0,4 do if (bits & (1 << (4-xx)))~=0 then img:drawPixel(x+xx,y+yy-1,color) end end end end;x=x+6 end
end
local PW,PH=416,536
local function review(f,bg)
 local img=Image(PW,PH,ColorMode.RGB);img:clear((bg=='#303843' and app.pixelColor.rgba(48,56,67,255) or app.pixelColor.rgba(164,197,175,255)))
 local ink=bg=='#303843' and app.pixelColor.rgba(202,230,217,255) or app.pixelColor.rgba(48,56,67,255)
 label(img,'SKIN ON',16,10,ink);label(img,'BUNCH',123,10,ink);label(img,'HALF PEELED',207,10,ink);label(img,'WORKER',342,10,ink)
 for i,data in ipairs(all) do
  local y=24+(i-1)*100
  img:drawImage(data.states[1].image,Point(0,y));img:drawImage(data.states[2].image,Point(104,y))
  img:drawImage(f and data.frames[f] or data.states[3].image,Point(208,y))
  img:drawImage(worker,Point(336,y+24));label(img,data.label,12,y+86,ink)
 end
 return img
end
local function enlarge(img,scale)
 local dst=Image(img.width*scale,img.height*scale,ColorMode.RGB)
 for y=0,dst.height-1 do for x=0,dst.width-1 do dst:drawPixel(x,y,img:getPixel(math.floor(x/scale),math.floor(y/scale))) end end;return dst
end
local native=review();native:saveAs(out..'banana-varieties-native-preview.png');enlarge(native,2):saveAs(out..'banana-varieties-preview.png')
-- Additional quiet dark ground demonstrates silhouette separation.
enlarge(review(nil,'#303843'),2):saveAs(out..'banana-varieties-dark-preview.png')
local gif=axi.new(PW*2,PH*2);gif:setPalette(pal);gif.layers[1].name='Review only'
for i,ms in ipairs(durations) do
 if i>1 then gif:newEmptyFrame() end;gif.frames[i].duration=(i==1 and 650 or (i==#durations and 1100 or ms))/1000
 gif:newCel(gif.layers[1],i,enlarge(review(i),2),Point(0,0))
end
gif:saveCopyAs(out..'banana-peeling-preview.gif');axi.save(out..'banana-peeling-preview.aseprite')
-- Contact sheet exposes every motion step, including the replay reset.
local contacts=Image(W*12,H*5,ColorMode.RGB);contacts:clear(app.pixelColor.rgba(164,197,175,255))
for row,data in ipairs(all) do for col,img in ipairs(data.frames) do contacts:drawImage(img,Point((col-1)*W,(row-1)*H)) end end
contacts:saveAs(out..'banana-peeling-contact.png')
print('PASS: '..checked..' source frames, binary alpha, palette, margins, exact atlas/sheets and reopened masters; 15 static sprites and 5 one-shot animations')
-- Generate matching looping shine states after the base exports.
dofile('tools/art/banana-idle.lua')
