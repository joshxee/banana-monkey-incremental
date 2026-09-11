"""Split the three collection bins out of the treehouse's structure sprite.

    python3 tools/art/split-bins.py        # from the repository root

Writes `town-center-bins.png` and rewrites `town-center-structure.png` without
the pixels it took, then checks that the three sprites drawn one over the other
are still the master, pixel for pixel - which is what `art.rs`'s
`the_treehouse_split_draws_exactly_the_artists_picture` holds at test time.

Python and Pillow rather than a `sprite-axi` recipe like its neighbours, because
this is not a drawing: it is a *partition* of an existing export, and the two
halves have to add back up to it exactly. It reads the bins' own geometry
straight out of `town-center.lua` - the same `bin(u, v, w, d, seed)` calls and
the same 2:1 ground projection - to decide which ground the bins are drawn on,
and the master's palette to decide which pixels on that ground are bin rather
than the trunk standing behind them. Re-run it after regenerating the town
centre from its Lua recipe; running it twice is a no-op, since a second run
finds no bin pixels left in the structure to take and writes nothing.

Requires Pillow (`pip install pillow`). It needs no Aseprite.
"""
from PIL import Image

OUT = 'assets/TownCenter/'

def P(u, v, z):
    return (330 + u - v, 440 + (u + v) / 2 - z)

# bin(u, v, w, d, seed), straight from the recipe.
BINS = [(90, 54, 46, 33, 1), (147, 71, 43, 34, 2), (131, 116, 49, 37, 3)]
BIN_HEIGHT = 27
# The bananas each bin carries, as the recipe places them.
BUNCHES = [(8, 7), (25, 7), ('w-10', 13), (11, 'd-9'), ('w-14', 'd-8'), (22, 16)]

BIN_COLOURS = {
    (0x40, 0x52, 0x73),  # blue: the body and its far faces
    (0x6c, 0x81, 0xa1),  # slate
    (0x96, 0xa9, 0xc1),  # mist: the lip
    (0x30, 0x38, 0x43),  # dark: the interior and the grip
    (0xde, 0x9f, 0x47),  # ochre, gold, cream: the fruit inside
    (0xfd, 0xd1, 0x79),
    (0xfe, 0xe1, 0xb8),
}
# Bark is the trunk's colour as well as a bunch's stem, so it comes across only
# from the few pixels the recipe puts a stem on.
STEM = (0x73, 0x4c, 0x44)

# The loose bunch the recipe drops beside the third bin - `banana(403, 585)` -
# overlaps its front corner, so it comes across with them: left behind it would
# bite a notch out of a bin the moment the two sprites are drawn apart.
FALLEN = (403, 585)


def polygon(points):
    """Every pixel of a filled polygon, by even-odd scanline."""
    ys = [p[1] for p in points]
    out = set()
    for y in range(int(min(ys)) - 1, int(max(ys)) + 2):
        crossings = []
        for i in range(len(points)):
            (x0, y0), (x1, y1) = points[i], points[(i + 1) % len(points)]
            if (y0 > y) != (y1 > y):
                crossings.append(x0 + (y - y0) * (x1 - x0) / (y1 - y0))
        crossings.sort()
        for a, b in zip(crossings[::2], crossings[1::2]):
            for x in range(int(a) - 1, int(b) + 2):
                out.add((x, y))
    return out


def dilate(mask, radius):
    grown = set()
    for x, y in mask:
        for dy in range(-radius, radius + 1):
            for dx in range(-radius, radius + 1):
                grown.add((x + dx, y + dy))
    return grown


def bin_mask():
    """The ground the bins and their fruit are drawn on, generously, and the
    handful of pixels a bunch's stem is allowed to claim."""
    mask, stems = set(), set()
    for u, v, w, d, _seed in BINS:
        # The box's silhouette: top, front and side faces, as `box` draws them.
        h = BIN_HEIGHT
        mask |= polygon([P(u, v, h), P(u + w, v, h), P(u + w, v + d, h), P(u, v + d, h)])
        mask |= polygon([P(u, v + d, h), P(u + w, v + d, h), P(u + w, v + d, 0), P(u, v + d, 0)])
        mask |= polygon([P(u + w, v, h), P(u + w, v + d, h), P(u + w, v + d, 0), P(u + w, v, 0)])
        # The lip and grip reach a pixel or two past the faces.
        mask = dilate(mask, 3)
        # And the loose bunches sit above the rim.
        for a, b in BUNCHES:
            au = w - 10 if a == 'w-10' else w - 14 if a == 'w-14' else a
            bv = d - 9 if b == 'd-9' else d - 8 if b == 'd-8' else b
            for z in range(30, 35):
                x, y = P(u + au, v + bv, z)
                x, y = int(x) - 5, int(y) - 4
                for dy in range(-8, 16):
                    for dx in range(-10, 20):
                        mask.add((x + dx, y + dy))
                for dy in range(-3, 4):
                    for dx in range(-3, 4):
                        stems.add((x + dx, y + dy))
    x, y = FALLEN
    for dy in range(-8, 16):
        for dx in range(-10, 20):
            mask.add((x + dx, y + dy))
    for dy in range(-3, 4):
        for dx in range(-3, 4):
            stems.add((x + dx, y + dy))
    return mask, stems


def main():
    whole = Image.open(OUT + 'town-center.png').convert('RGBA')
    structure = Image.open(OUT + 'town-center-structure.png').convert('RGBA')
    width, height = whole.size
    mask, stems = bin_mask()
    source = structure.load()
    bins = Image.new('RGBA', (width, height), (0, 0, 0, 0))
    kept = Image.new('RGBA', (width, height), (0, 0, 0, 0))
    moved = 0
    for y in range(height):
        for x in range(width):
            pixel = source[x, y]
            if pixel[3] == 0:
                continue
            bin_pixel = pixel[:3] in BIN_COLOURS or (
                pixel[:3] == STEM and (x, y) in stems
            )
            if (x, y) in mask and bin_pixel:
                bins.putpixel((x, y), pixel)
                moved += 1
            else:
                kept.putpixel((x, y), pixel)
    if moved == 0:
        print('the structure carries no bin pixels: already split, nothing written')
        return
    bins.save(OUT + 'town-center-bins.png')
    kept.save(OUT + 'town-center-structure.png')
    box = bins.getbbox()
    print(f'{moved} pixels moved; bins bounds {box}')

    # The three sprites, drawn bins over structure over ground, must still be
    # the artist's picture.
    ground = Image.open(OUT + 'town-center-ground.png').convert('RGBA')
    layers = [bins.load(), kept.load(), ground.load()]
    want = whole.load()
    wrong = 0
    for y in range(height):
        for x in range(width):
            drawn = (0, 0, 0, 0)
            for layer in layers:
                if layer[x, y][3] > 0:
                    drawn = layer[x, y]
                    break
            if drawn != want[x, y]:
                wrong += 1
    print(f'{wrong} pixels differ from the master')


main()
