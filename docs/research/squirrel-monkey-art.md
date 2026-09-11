# Squirrel monkey unpacker: real-animal reference

Researched 2026-09-11 for the unpacker asset: dart to a spider worker, receive bananas, return to the unload facility, and drop the bananas into its boxes. No existing research-note folder was present; this note starts `docs/research/`.

## Appearance to preserve

| Real feature | Implication for pixel art |
| --- | --- |
| White fur surrounds the eyes, ears, throat, and neck sides; the exposed nose/lip area is black. The crown is gray to black. [Oakland Zoo](https://www.oaklandzoo.org/animals/squirrel-monkey/) | Make the pale two-lobed face mask and compact dark muzzle the strongest small-scale identifying marks. Keep the eyes distinct from the muzzle. |
| Common squirrel monkeys have gray-green to auburn bodies and yellow/orange hands, feet, and forearms. Their white brow markings rise higher than the rounded brow of Bolivian squirrel monkeys. [Wisconsin National Primate Research Center](https://primate.wisc.edu/primate-info-net/pin-factsheets/pin-factsheet-squirrel-monkey/) | Use a muted olive-gray back, warm golden limbs, pale belly, and a darker cap with a small central dip. These choices suggest a common squirrel monkey rather than a generic brown monkey. |
| Fur is short and dense; the tail is longer than the body and ends in black. The tail is non-prehensile and provides balance during leaps. [Oakland Zoo](https://www.oaklandzoo.org/animals/squirrel-monkey/) | Use a slender trailing tail with a dark tip. No squirrel-like bushy tail, spiral gripping pose, or bananas held by the tail. |
| Bolivian squirrel monkeys have large eyes and ears, a black muzzle, yellow or reddish limbs, and a pale underside. [Taipei Zoo](https://www.zoo.gov.taipei/News_Content.aspx?n=468E81F307502E43&s=03DB42AE35B7EE1B&sms=782334A7A9EFEEAC) | Slightly enlarged eyes and compact rounded ears are compatible with the game's playful proportions. Avoid huge mouse ears. |

## Size and movement

Oakland Zoo gives a squirrel monkey body length of 9–14 inches (23–36 cm), a 14–17 inch (36–43 cm) tail, and a mass of 1.3–2.5 pounds (0.6–1.1 kg). [Oakland Zoo](https://www.oaklandzoo.org/animals/squirrel-monkey/)

For comparison, LA Zoo gives Geoffroy's spider monkeys a 14–20 inch (36–51 cm) body and a 22–33 inch (56–84 cm) tail; it describes their long, slim limbs and gripping tail. [Los Angeles Zoo](https://lazoo.org/explore-your-zoo/our-animals/mammals/geoffroys-spider-monkey/) The game should communicate a smaller, more compact unpacker beside the lanky spider worker. An exact sprite-height ratio is an art decision, because stance and limb proportions affect apparent height.

Squirrel monkeys normally move quadrupedally along branches and rarely descend to the ground. [Wisconsin National Primate Research Center](https://primate.wisc.edu/primate-info-net/pin-factsheets/pin-factsheet-squirrel-monkey/) Their natural leaping and balancing tail support a quick, low darting animation. [Oakland Zoo](https://www.oaklandzoo.org/animals/squirrel-monkey/) A ground courier that carries a banana bundle in both hands is deliberate game stylization, not a documented natural behavior.

## Animation brief (art direction, not zoological claims)

- **Idle:** alert crouch, small breathing change, occasional blink; keep the tail mostly steady.
- **Empty dart:** forward lean, compact alternating hand/foot contacts, brief suspended stride, trailing tail. Avoid a slow upright march.
- **Receive:** brake, plant feet, lift both hands toward the spider monkey, then settle the visible banana bundle against the chest.
- **Loaded dart:** use a shorter bipedal scamper to keep both hands around the bundle. Keep the face above the bananas and the golden hands distinguishable from the fruit.
- **Drop:** lean over the box rim, extend hands, release the banana bundle downward, and recover empty. The bananas should visibly cross from hands into the box.
- Supply opposite-facing motion and consistent foot anchors so a route can reverse without changing scale or popping vertically. Keep bananas separable if the renderer needs to synchronize receipt and release with game state.

The cited zoo and research-center pages include real-animal photographs for visual reference. They are references, not game assets or an implied license to redistribute those photographs.
