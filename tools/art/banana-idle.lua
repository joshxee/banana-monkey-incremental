-- Reference-inspired idle: one-pixel lift; only shiny gets a moving highlight.
-- Called after banana-varieties.lua; can run standalone after base generation.
local out='assets/Banana/Varieties/'
local function rgba(h) return app.pixelColor.rgba(tonumber(h:sub(2,3),16),tonumber(h:sub(4,5),16),tonumber(h:sub(6,7),16),255) end
local allowed={};local pi=Image{fromFile=out..'banana-palette.png'}
for x=0,pi.width-1 do allowed[pi:getPixel(x,0)]=true end
local variants={
 {id='yellow',colors={'#b87828','#f3be32','#ffdc52'},light='#fff08a'},
 {id='blue',colors={'#2666a0','#398ed8','#80d6f3'},light='#c0f3ff'},
 {id='red',colors={'#94374b','#db5361','#ff9382'},light='#ffc1a1'},
 {id='huge',colors={'#b87828','#f3be32','#ffdc52'},light='#fff08a'},
 {id='shiny',colors={'#de9f47','#ffdc52','#fff08a'},light='#fff9e8'}
}
local forms={{id='single',suffix=''},{id='bunch',suffix='-bunch'},{id='half-peeled',suffix='-half-peeled'}}
local durations={180,100,100,100,120,120,120,120,120,120,120,280}
local offsets={0,-1,-1,0,0,0,0,0,0,0,0,0}
local function flat(s,f) local img=Image(96,96,ColorMode.RGB);img:drawSprite(s,f);return img end
local function equal(a,b) for y=0,a.height-1 do for x=0,a.width-1 do if a:getPixel(x,y)~=b:getPixel(x,y) then return false end end end;return true end
local clips={};local shiny={}
for _,v in ipairs(variants) do for _,form in ipairs(forms) do
 local basename='banana-'..v.id..form.suffix;local s=app.open(out..basename..'.aseprite');app.activeSprite=s
 local base=flat(s,1);local layers={}
 for _,l in ipairs(s.layers) do local c=l:cel(1);if c then layers[#layers+1]={layer=l,image=Image(c.image),pos=c.position} end end
 local mask={};for _,c in ipairs(v.colors) do mask[rgba(c)]=true end
 local minY,maxY=96,-1
 for y=0,95 do for x=0,95 do if mask[base:getPixel(x,y)] then minY=math.min(minY,y);maxY=math.max(maxY,y) end end end
 local effects=s:newLayer();effects.name='09 Moving peel highlight';local frames={};local effectChanges=0
 for f,ms in ipairs(durations) do
  if f>1 then s:newEmptyFrame();for _,b in ipairs(layers) do s:newCel(b.layer,f,b.image,Point(b.pos.x,b.pos.y+offsets[f])) end end
  s.frames[f].duration=ms/1000;local fx=Image(96,96,ColorMode.RGB)
  -- Mirror the reference's descending light patch, without external stars.
  if v.id=='shiny' and f>=4 and f<=10 then
   local sweep=minY+(maxY-minY)*(f-4)/6
   for y=0,95 do for x=0,95 do
    if mask[base:getPixel(x,y)] then
     local d=y-sweep+(x-48)*.18
     local band=v.id=='shiny' and 2.3 or 1.1
     if d>=0 and d<band then fx:drawPixel(x,y+offsets[f],rgba(v.light));effectChanges=effectChanges+1 end
    end
   end end
  end
  s:newCel(effects,f,fx,Point(0,0));frames[f]=flat(s,f)
  for y=0,95 do for x=0,95 do
   local p=frames[f]:getPixel(x,y);local a=app.pixelColor.rgbaA(p)
   assert(a==0 or a==255,'Idle partial alpha');if a>0 then assert(allowed[p],'Idle palette');assert(x>1 and x<94 and y>1 and y<94,'Idle clipping') end
   local sourceY=y-offsets[f];local original=sourceY>=0 and sourceY<96 and base:getPixel(x,sourceY) or 0
   assert(app.pixelColor.rgbaA(p)==app.pixelColor.rgbaA(original),'Idle silhouette changed beyond one-pixel lift')
   if app.pixelColor.rgbaA(fx:getPixel(x,y))==0 then assert(p==original,'Unexpected idle pixel change') end
  end end
 end
 assert(equal(frames[1],base) and equal(frames[12],base),'Idle loop/static seam')
 assert(not equal(frames[2],base),'Idle must move')
 if v.id=='shiny' then assert(effectChanges>0,'Shiny must shine')
 else assert(effectChanges==0,'Non-shiny must not shine');for f=4,12 do assert(equal(frames[f],base),'Non-shiny rest must equal static') end end
 local tag=s:newTag(1,12);tag.name='idle'
 local name=basename..'-idle';s:saveAs(out..name..'.aseprite')
 local disk=app.open(out..name..'.aseprite')
 assert(#disk.frames==12 and disk.tags[1].name=='idle' and disk.tags[1].toFrame.frameNumber==12,'Idle tag')
 for f,img in ipairs(frames) do assert(equal(img,flat(disk,f)),'Idle saved master');assert(math.abs(disk.frames[f].duration*1000-durations[f])<.01,'Idle timing') end
 disk:close();app.activeSprite=s
 local sheet=Image(1152,96,ColorMode.RGB);for f,img in ipairs(frames) do sheet:drawImage(img,Point((f-1)*96,0)) end
 sheet:saveAs(out..name..'-sheet.png');assert(equal(sheet,Image{fromFile=out..name..'-sheet.png'}),'Idle sheet mismatch')
 local clip={id=v.id..'-'..form.id,variety=v.id,form=form.id,name=name,frames=frames};clips[#clips+1]=clip
 if v.id=='shiny' then
  -- Preserve existing shine filenames as aliases of the new reference-style idle.
  local alias=basename..'-shine';s:saveCopyAs(out..alias..'.aseprite');sheet:saveAs(out..alias..'-sheet.png')
  shiny[#shiny+1]={id=form.id,variety=v.id,form=form.id,name=alias,frames=frames}
 end
end end
local function metadata(list,filename,global)
 local jf=assert(io.open(out..filename..'.json','w'))
 jf:write('{"cellWidth":96,"cellHeight":96,"anchor":{"x":48,"y":80},"sampling":"nearest","animations":{\n')
 for i,clip in ipairs(list) do
  jf:write(string.format('"%s":{"variety":"%s","form":"%s","source":"%s.aseprite","image":"%s-sheet.png","tag":"idle","loop":true,"durationMs":1600,"asepriteFrames":[1,12],"frames":[',clip.id,clip.variety,clip.form,clip.name,clip.name))
  for f,ms in ipairs(durations) do jf:write(string.format('%s{"x":%d,"y":0,"w":96,"h":96,"durationMs":%d,"offsetY":%d}',f>1 and ',' or '',(f-1)*96,ms,offsets[f])) end
  jf:write(']}'..(i<#list and ',' or '')..'\n')
 end
 jf:write('}}\n');jf:close()
 local read=assert(io.open(out..filename..'.json','r'));local data=read:read('*a');read:close()
 local js=assert(io.open(out..filename..'-data.js','w'));js:write('window.'..global..' = '..data..';\n');js:close()
end
metadata(clips,'banana-idle','BANANA_IDLE');metadata(shiny,'banana-shiny-shine','BANANA_SHINE')
local function preview(list,columns,scale,name)
 local rows=math.ceil(#list/columns);local w,h=columns*96,rows*96
 local s=axi.new(w*scale,h*scale);local contact=Image(1152,#list*96,ColorMode.RGB);contact:clear(rgba('#303843'))
 for f,ms in ipairs(durations) do
  if f>1 then s:newEmptyFrame() end;s.frames[f].duration=ms/1000
  local img=Image(w*scale,h*scale,ColorMode.RGB);img:clear(rgba('#303843'))
  for i,clip in ipairs(list) do
   contact:drawImage(clip.frames[f],Point((f-1)*96,(i-1)*96))
   local ox=((i-1)%columns)*96*scale;local oy=math.floor((i-1)/columns)*96*scale
   for y=0,96*scale-1 do for x=0,96*scale-1 do local p=clip.frames[f]:getPixel(math.floor(x/scale),math.floor(y/scale));if app.pixelColor.rgbaA(p)>0 then img:drawPixel(ox+x,oy+y,p) end end end
  end
  s:newCel(s.layers[1],f,img,Point(0,0))
 end
 s:saveCopyAs(out..name..'-preview.gif');axi.save(out..name..'-preview.aseprite');contact:saveAs(out..name..'-contact.png')
end
preview(clips,3,2,'banana-idle');preview(shiny,3,3,'banana-shiny-shine')
print('PASS: 15 idle loops / 180 frames; 1600ms, one-pixel lift only, peel-only shine, no external stars, palette/alpha/margins/master/sheet/timing/static-loop seams')
