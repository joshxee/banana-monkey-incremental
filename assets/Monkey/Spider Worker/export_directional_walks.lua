-- sprite-axi run this-file -f spider_monkey_directional_walks.aseprite
local root="assets/Monkey/Spider Worker/"
local s=app.activeSprite
assert(s.width==64 and s.height==64 and #s.frames==192)
local dirs={"N","NE","E","SE","S","SW","W","NW"}
local images={}
for f=1,192 do
  local img=Image(64,64,ColorMode.RGB);img:drawSprite(s,f);images[f]=img
end
local function png(img,name) img:saveAs(root..name) end
for state=0,1 do
  local sheet=Image(768,512,ColorMode.RGB)
  for d=0,7 do for phase=0,11 do
    sheet:drawImage(images[state*96+d*12+phase+1],Point(phase*64,d*64))
  end end
  png(sheet,state==0 and "spider_monkey_walk_8dir.png" or "spider_monkey_carry_walk_8dir.png")
end
local review=axi.new(1024,256,{mode="rgb"})
review:deleteLayer(review.layers[1]);axi.frames(11)
local layer=axi.layer("Review only - sand background")
for phase=0,11 do
  local c=axi.cel(layer,phase+1)
  axi.rect(c,0,0,1024,256,"#D4C692",{fill=true})
  for state=0,1 do for d=0,7 do
    local img=images[state*96+d*12+phase+1]
    for y=0,63 do for x=0,63 do
      local pixel=img:getPixel(x,y)
      if app.pixelColor.rgbaA(pixel)>0 then
        for yy=0,1 do for xx=0,1 do
          c.image:drawPixel(d*128+x*2+xx,state*128+y*2+yy,pixel)
        end end
      end
    end end
  end end
  axi.duration(phase+1,({70,60,50})[phase%3+1])
end
axi.save(root.."spider_monkey_8dir_preview.gif")
local still=Image(1024,256,ColorMode.RGB);still:drawSprite(review,1)
png(still,"spider_monkey_8dir_preview.png")
local metadata={
  version=1,master="spider_monkey_directional_walks.aseprite",
  cell={width=64,height=64},anchor={x=32,y=56},
  directions=dirs,directionConvention="Screen compass: N is up; diagonal ground axes are 2:1.",
  mirroredAboutX=32,frameDurationMs={70,60,50,70,60,50,70,60,50,70,60,50},
  loopDurationMs=720,framesPerClip=12,loop=true,clips={}
}
for state=0,1 do for d=0,7 do
  local frames={}
  for phase=0,11 do frames[#frames+1]={x=phase*64,y=d*64,w=64,h=64} end
  metadata.clips[#metadata.clips+1]={
    name=(state==0 and "walk_" or "carry_walk_")..dirs[d+1],
    sheet=state==0 and "spider_monkey_walk_8dir.png" or "spider_monkey_carry_walk_8dir.png",
    direction=dirs[d+1],state=state==0 and "walk" or "carry_walk",row=d,
    asepriteFirstFrame=state*96+d*12+1,asepriteLastFrame=state*96+d*12+12,frames=frames
  }
end end
local f=assert(io.open(root.."spider_monkey_directional_walks.json","w"))
f:write(json.encode(metadata));f:write("\n");f:close()
print("Exported two 768x512 sheets, 16 clips, 192 frames, 720ms loops, common anchor (32,56).")
