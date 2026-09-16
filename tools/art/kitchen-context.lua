-- Static composition with main's approved baboons and grill; not a screenshot.
local s=axi.new(440,380);s:deleteLayer(s.layers[1]);local bg=axi.cel('Quiet ground',1);axi.rect(bg,0,0,440,380,'#bcad9f',{fill=true})
local floor=axi.cel('Kitchen floor below every chef',1);floor.image:drawImage(Image{fromFile='assets/Expansion/chef-kitchen-ground.png'},Point(60,0))
local props=axi.cel('Peripheral preparation counters',1);props.image:drawImage(Image{fromFile='assets/Expansion/chef-kitchen-structure.png'},Point(60,0))
local crew=axi.cel('Approved grill and baboon chefs from main',1);crew.image:drawImage(Image{fromFile='assets/Monkey/Baboon Chef/v2/grill-3-chefs.png'},Point(108,179))
axi.save('assets/Expansion/kitchen-context.aseprite');s:saveCopyAs('assets/Expansion/kitchen-context.png')
