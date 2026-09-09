-- sprite-axi run "assets/Monkey/Spider Worker/create_preview.lua" -f "assets/Monkey/Spider Worker/spider_monkey_idle.ase"
-- Presentation only: 1x and 4x views on both game background colours.
local source = Image(64,64,ColorMode.RGB)
source:drawSprite(app.activeSprite,1)
local preview = axi.new(640,320,{mode="rgb"})
local cel = axi.cel(axi.layer("Preview"),1)
axi.rect(cel,0,0,320,320,"#819447",{fill=true})
axi.rect(cel,320,0,320,320,"#D4C692",{fill=true})
for side=0,1 do
  cel.image:drawImage(source,Point(side*320+8,248))
  for y=0,63 do
    for x=0,63 do
      local pixel=source:getPixel(x,y)
      if app.pixelColor.rgbaA(pixel)>0 then
        for dy=0,3 do
          for dx=0,3 do cel.image:drawPixel(side*320+32+x*4+dx,8+y*4+dy,pixel) end
        end
      end
    end
  end
end
axi.save("assets/Monkey/Spider Worker/preview.png")
