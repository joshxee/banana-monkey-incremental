-- Verify all nine current masters without redrawing them; write machine-readable anchors.
local names={'fan-palm','buttress-tree','bamboo','vine-tree','fallen-log','deep-jungle-floor','research-hut','distribution-center','chef-kitchen'}
local p=Image{fromFile='docs/references/palette.png'};local allowed={};for x=0,30 do allowed[p:getPixel(x,0)]=true end
local entries={}
for _,name in ipairs(names) do
 local spr=app.open('assets/Expansion/'..name..'.aseprite');local flat=Image(spr.width,spr.height);flat:drawSprite(spr,1)
 local png=Image{fromFile='assets/Expansion/'..name..'.png'};assert(png.width==spr.width and png.height==spr.height)
 local minx,miny,maxx,maxy=spr.width,spr.height,-1,-1
 for y=0,spr.height-1 do for x=0,spr.width-1 do local v=flat:getPixel(x,y);assert(png:getPixel(x,y)==v,'master mismatch '..name);local a=app.pixelColor.rgbaA(v);assert(a==0 or a==255,'alpha');if a>0 then assert(allowed[v],'palette');minx=math.min(minx,x);miny=math.min(miny,y);maxx=math.max(maxx,x);maxy=math.max(maxy,y) end end end
 assert(maxx>=minx)
 local floor=name=='deep-jungle-floor'
 if not floor then assert(minx>0 and miny>0 and maxx<spr.width-1 and maxy<spr.height-1) end
 entries[#entries+1]=string.format('    {"name":"%s","canvas":[%d,%d],"ground_anchor":[%d,%d],"opaque_bounds":[%d,%d,%d,%d],"layers":%d}',name,spr.width,spr.height,floor and 64 or 160,floor and 32 or 316,minx,miny,maxx,maxy,#spr.layers)
 print(name..': master/export, palette, alpha and margins PASS');spr:close()
end
local f=io.open('assets/Expansion/manifest.json','w');f:write('{\n  "art_scale":0.5,\n  "static":true,\n  "assets":[\n'..table.concat(entries,',\n')..'\n  ]\n}\n');f:close()
