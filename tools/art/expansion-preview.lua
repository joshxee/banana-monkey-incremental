-- Reproducible native-pixel art contact sheet; not a runtime screenshot.
local names={'fan-palm','buttress-tree','bamboo','vine-tree','fallen-log','deep-jungle-floor','research-hut','distribution-center','chef-kitchen'}
local s=axi.new(1200,1120);s:deleteLayer(s.layers[1]);local cel=axi.cel('Asset comparisons at native pixels',1)
local bg=Color{r=164,g=197,b=175,a=255}.rgbaPixel;cel.image:clear(bg)
for i,name in ipairs(names) do local x=((i-1)%3)*400;local y=math.floor((i-1)/3)*372
 cel.image:drawImage(Image{fromFile='assets/Expansion/'..name..'-preview.png'},Point(x,y+20))
end
axi.save('assets/Expansion/contact-sheet.aseprite');s:saveCopyAs('assets/Expansion/contact-sheet.png')
