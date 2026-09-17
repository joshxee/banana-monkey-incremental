local src=Image{fromFile='docs/references/bananas/idle-reference.png'}
local s=axi.new(1152,96);local c=axi.cel('Reference enlarged for inspection',1)
for y=0,95 do for x=0,1151 do c.image:drawPixel(x,y,src:getPixel(math.floor(x/6),math.floor(y/6))) end end
axi.save('docs/references/bananas/idle-reference-enlarged.png')
for f=0,11 do local bounds={16,16,-1,-1};local count=0
for y=0,15 do for x=0,15 do if app.pixelColor.rgbaA(src:getPixel(f*16+x,y))>0 then count=count+1;bounds[1]=math.min(bounds[1],x);bounds[2]=math.min(bounds[2],y);bounds[3]=math.max(bounds[3],x);bounds[4]=math.max(bounds[4],y) end end end
print(f..': '..table.concat(bounds,',')..'; '..count..'px') end
