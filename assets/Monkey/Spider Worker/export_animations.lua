-- sprite-axi run "assets/Monkey/Spider Worker/export_animations.lua" -f "assets/Monkey/Spider Worker/spider_monkey_animations.ase"
-- Fixed-cell horizontal PNG strips and looping 4x GIF previews on sand.
local source = app.activeSprite
local palette = axi.getPalette()
local root = "assets/Monkey/Spider Worker/"
local function preview(name,indices)
  local result = axi.new(256,256,{mode="rgb"})
  result:deleteLayer(result.layers[1])
  axi.frames(#indices-1)
  local layer = axi.layer(name)
  for f,index in ipairs(indices) do
    local img = Image(64,64,ColorMode.RGB)
    img:drawSprite(source,index)
    local c = axi.cel(layer,f)
    axi.rect(c,0,0,256,256,"#D4C692",{fill=true})
    for y=0,63 do for x=0,63 do
      local pixel = img:getPixel(x,y)
      if app.pixelColor.rgbaA(pixel)>0 then
        for dy=0,3 do for dx=0,3 do c.image:drawPixel(x*4+dx,y*4+dy,pixel) end end
      end
    end end
    result.frames[f].duration = source.frames[index].duration
  end
  axi.save(root.."spider_monkey_"..name.."_preview.gif")
end
local clips={}
for _,tag in ipairs(source.tags) do
  clips[#clips+1]={tag.name,tag.fromFrame.frameNumber,tag.toFrame.frameNumber}
end
for _,clip in ipairs(clips) do
  local name,first,last = table.unpack(clip)
  local count = last-first+1
  local images = {}
  for f=first,last do
    local img = Image(64,64,ColorMode.RGB)
    img:drawSprite(source,f)
    images[#images+1] = img
  end
  local strip = axi.new(64*count,64,{mode="rgb"})
  strip:deleteLayer(strip.layers[1])
  axi.setPalette(palette)
  local cel = axi.cel(axi.layer(name),1)
  for f,img in ipairs(images) do cel.image:drawImage(img,Point((f-1)*64,0)) end
  axi.save(root.."spider_monkey_"..name.."_sheet.png")

  if name=="idle" or name=="walk" then
    local indices={}
    for f=first,last do indices[#indices+1]=f end
    preview(name,indices)
  end
end
local sequence={}
for _,name in ipairs({"idle","rise","walk","walk","walk","settle"}) do
  for _,clip in ipairs(clips) do
    if clip[1]==name then for f=clip[2],clip[3] do sequence[#sequence+1]=f end end
  end
end
preview("gait",sequence)
