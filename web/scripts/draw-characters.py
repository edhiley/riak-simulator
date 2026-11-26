"""Draws the simulator's pixel characters. Writes a preview PNG,
src/lib/characters.ts and the colour token lines for make-tokens.ts.

Run from web/: python3 scripts/draw-characters.py /tmp/preview.png src/lib/characters.ts /tmp/tokens.txt
Needs Pillow for the preview."""
import sys, math, json
from PIL import Image

W = H = 24
PAL = {
    "k": ("ink", "#2a2238"),
    "s": ("skin", "#f3c9a2"),
    "S": ("skin-shade", "#d9a07e"),
    "h": ("hair", "#3b2b2f"),
    "w": ("white", "#fffaf0"),
    "p": ("blush", "#f29b9b"),
    "y": ("gold", "#f4b942"),
    "Y": ("gold-dark", "#c8892a"),
    "b": ("wood", "#a06e45"),
    "B": ("wood-dark", "#6b4429"),
    "P": ("paper", "#f7ecd2"),
    "Q": ("paper-shade", "#dfcda5"),
    "g": ("grey", "#b3adc2"),
    "G": ("grey-dark", "#6f6880"),
    "r": ("red", "#de5a4f"),
    "R": ("red-dark", "#a63a36"),
    "i": ("indigo", "#4a66b8"),
    "I": ("indigo-dark", "#2f4382"),
    "t": ("teal", "#33a89d"),
    "T": ("teal-dark", "#217a71"),
    "m": ("plum", "#a45fb2"),
    "M": ("plum-dark", "#713f80"),
    "o": ("ochre", "#e8a040"),
    "O": ("ochre-dark", "#b5752b"),
    "e": ("green", "#5fb066"),
    "E": ("green-dark", "#3d7d44"),
    "n": ("navy", "#34416a"),
    "N": ("navy-dark", "#222b48"),
    "c": ("glass", "#a8dcf2"),
    "l": ("lantern", "#ffd36b"),
}


class G:
    def __init__(self):
        self.g = [["." for _ in range(W)] for _ in range(H)]

    def px(self, x, y, c):
        if 0 <= x < W and 0 <= y < H:
            self.g[y][x] = c

    def rect(self, x0, y0, x1, y1, c):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.px(x, y, c)

    def ell(self, cx, cy, rx, ry, c, top=None, bottom=None):
        for y in range(H):
            for x in range(W):
                if ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1.0:
                    if top is not None and y > top:
                        continue
                    if bottom is not None and y < bottom:
                        continue
                    self.px(x, y, c)

    def line(self, x0, y0, x1, y1, c):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            self.px(round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n), c)

    def outline(self):
        out = [row[:] for row in self.g]
        for y in range(H):
            for x in range(W):
                if self.g[y][x] != ".":
                    continue
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    X, Y = x + dx, y + dy
                    if 0 <= X < W and 0 <= Y < H and self.g[Y][X] not in (".", "k"):
                        out[y][x] = "k"
                        break
        self.g = out

    def rows(self):
        return ["".join(r) for r in self.g]


def head(g, hair="h", cx=12, cy=7, hat=None):
    g.ell(cx, cy, 5.2, 4.8, "s")
    g.ell(cx, cy - 1, 5.4, 4.6, hair, top=cy - 2)
    g.px(cx - 5, cy, hair)
    g.px(cx + 5, cy, hair)
    # eyes, blush, mouth
    g.px(cx - 2, cy + 1, "k")
    g.px(cx + 2, cy + 1, "k")
    g.px(cx - 3, cy + 2, "p")
    g.px(cx + 3, cy + 2, "p")
    g.px(cx, cy + 3, "S")


def body(g, main, shade, belt=None, cx=12):
    g.rect(cx - 4, 12, cx + 3, 18, main)
    g.rect(cx + 2, 12, cx + 3, 18, shade)
    g.px(cx - 1, 12, "w")
    g.px(cx, 13, "w")
    if belt:
        g.rect(cx - 4, 16, cx + 3, 16, belt)


def legs(g, c="N", step=0, cx=12):
    if step == 0:
        g.rect(cx - 3, 19, cx - 2, 21, c)
        g.rect(cx + 1, 19, cx + 2, 21, c)
        g.rect(cx - 4, 22, cx - 2, 22, "B")
        g.rect(cx + 1, 22, cx + 3, 22, "B")
    elif step == 1:
        g.rect(cx - 4, 19, cx - 3, 20, c)
        g.rect(cx - 6, 21, cx - 4, 21, "B")
        g.rect(cx + 2, 19, cx + 3, 21, c)
        g.rect(cx + 2, 22, cx + 4, 22, "B")
    else:
        g.rect(cx - 3, 19, cx - 2, 21, c)
        g.rect(cx - 4, 22, cx - 2, 22, "B")
        g.rect(cx + 2, 19, cx + 3, 20, c)
        g.rect(cx + 4, 21, cx + 6, 21, "B")


def arm(g, x0, y0, x1, y1, c):
    g.line(x0, y0, x1, y1, c)
    g.line(x0 + 1, y0, x1 + 1, y1, c)
    g.px(x1, y1 + 1, "s")
    g.px(x1 + 1, y1 + 1, "s")


# --- characters --------------------------------------------------------

def bookie(f):
    """A merchant at the counter with a soroban (abacus) and a ledger."""
    g = G()
    head(g)
    g.rect(9, 2, 15, 2, "i")  # headband
    body(g, "i", "I", belt="y")
    arm(g, 7, 13, 6, 16, "i")
    arm(g, 16, 13, 17, 16, "I")
    # counter
    g.rect(1, 18, 22, 19, "b")
    g.rect(1, 20, 22, 23, "B")
    g.rect(3, 21, 20, 21, "b")
    # ledger book, open
    g.rect(2, 15, 9, 17, "P")
    g.px(5, 15, "Q"); g.px(5, 16, "Q"); g.px(5, 17, "Q")
    g.line(3, 16, 4, 16, "G")
    g.line(6, 16 if f == 0 else 15, 8, 16 if f == 0 else 15, "G")
    # soroban
    g.rect(14, 15, 22, 17, "B")
    for x in range(15, 22, 2):
        g.px(x if f == 0 else x + (1 if x % 4 == 1 else 0), 16, "y")
    return g


def inker(f):
    """A calligrapher with a brush over a long scroll."""
    g = G()
    head(g)
    g.rect(11, 1, 13, 2, "h")  # top knot
    body(g, "m", "M", belt="y")
    arm(g, 7, 13, 6, 17, "m")
    # brush arm
    if f == 0:
        arm(g, 16, 13, 18, 14, "M")
        g.line(20, 10, 20, 15, "b")
        g.px(20, 16, "k")
    else:
        arm(g, 16, 13, 18, 16, "M")
        g.line(19, 13, 19, 18, "b")
        g.px(19, 19, "k")
    # scroll
    g.rect(1, 20, 22, 22, "P")
    g.rect(1, 19, 1, 23, "B")
    g.rect(22, 19, 22, 23, "B")
    g.px(4, 21, "k"); g.px(5, 21, "k"); g.px(8, 21, "k"); g.px(11, 21, "k"); g.px(12, 21, "k")
    if f == 1:
        g.px(16, 21, "k"); g.px(17, 21, "k"); g.px(19, 21, "k")
    return g


def iclerk(f):
    """A sweeper with a tenugui headscarf and a bamboo broom."""
    g = G()
    head(g)
    g.ell(12, 5, 5.6, 3.2, "t", top=5)  # scarf
    g.px(17, 6, "t"); g.px(18, 7, "T")
    body(g, "t", "T", belt="P")
    g.rect(8, 15, 14, 18, "P")  # apron
    legs(g, "T", 0)
    arm(g, 7, 13, 6, 16, "t")
    x = 18 if f == 0 else 16
    g.line(x + 1, 6, x + 2, 18, "y")
    g.rect(x, 19, x + 5, 20, "Y")
    g.line(x, 21, x + 5, 21, "Y")
    arm(g, 16, 13, x, 14, "T")
    if f == 1:
        g.px(21, 22, "g"); g.px(23, 21, "g"); g.px(22, 23, "g")
    return g


def penciller(f):
    """A scholar with round glasses, a pencil and an index card."""
    g = G()
    head(g)
    g.px(9, 8, "c"); g.px(10, 8, "k"); g.px(11, 8, "c")
    g.px(13, 8, "c"); g.px(14, 8, "k"); g.px(15, 8, "c")
    g.px(12, 8, "G")
    body(g, "o", "O", belt="O")
    legs(g, "O", 0)
    # card held in left hand
    g.rect(2, 12, 7, 16, "P")
    g.line(3, 13, 6, 13, "Q")
    g.line(3, 15, 5 if f == 0 else 6, 15, "G")
    arm(g, 7, 13, 6, 15, "o")
    # pencil hand
    if f == 0:
        arm(g, 16, 13, 17, 15, "O")
        g.line(18, 10, 18, 15, "y"); g.px(18, 9, "r"); g.px(18, 16, "k")
    else:
        arm(g, 16, 13, 14, 14, "O")
        g.line(9, 11, 13, 14, "y"); g.px(8, 10, "r"); g.px(7, 15, "k")
    return g


def pclerk(f):
    """A porter with a hachimaki headband, carrying a crate."""
    g = G()
    head(g)
    g.rect(7, 4, 17, 4, "w")
    g.px(18, 5, "w"); g.px(19, 6, "r")
    body(g, "e", "E", belt="w")
    legs(g, "E", 0 if f == 0 else 1)
    arm(g, 7, 13, 6, 16, "e")
    y = 8 if f == 0 else 10
    g.rect(15, y, 22, y + 6, "b")
    g.rect(15, y, 22, y, "B")
    g.line(15, y + 3, 22, y + 3, "B")
    g.px(18, y + 1, "P"); g.px(19, y + 1, "P")
    arm(g, 16, 13, 16, y + 5, "E")
    return g


def doorman(f):
    """A monban gate guard: a staff, and a paper lantern."""
    g = G()
    head(g)
    g.rect(8, 2, 16, 3, "n")  # cap
    g.rect(7, 4, 17, 4, "N")
    body(g, "n", "N", belt="y")
    legs(g, "N", 0)
    # staff
    g.line(4, 3, 4, 22, "B")
    g.rect(3, 2, 5, 3, "g")
    arm(g, 7, 13, 5, 14, "n")
    if f == 0:
        # lantern held low
        arm(g, 16, 13, 17, 16, "N")
        g.ell(19, 20, 2.2, 2.6, "l")
        g.rect(18, 17, 20, 17, "k")
        g.line(18, 20, 20, 20, "r")
    else:
        # hand raised: stop
        arm(g, 16, 12, 18, 6, "N")
        g.rect(18, 4, 20, 6, "s")
        g.ell(20, 20, 2.2, 2.6, "l")
        g.rect(19, 17, 21, 17, "k")
        g.line(19, 20, 21, 20, "r")
    return g


def movers(f):
    """A mover pulling a daihachi handcart loaded with boxes."""
    g = G()
    cx = 17
    head(g, cx=cx, cy=7)
    g.rect(cx - 5, 4, cx + 5, 4, "w")
    body(g, "o", "O", belt="w", cx=cx)
    legs(g, "O", 1 if f == 0 else 2, cx=cx)
    # cart bed and two boxes
    g.rect(0, 15, 9, 16, "B")
    g.rect(0, 10, 4, 14, "b")
    g.rect(0, 10, 4, 10, "B")
    g.rect(5, 12, 9, 14, "y")
    g.rect(5, 12, 9, 12, "Y")
    g.rect(1, 6, 3, 9, "P")
    # shaft to the mover's hands
    g.line(9, 15, cx - 5, 15, "B")
    # wheel, turning
    g.ell(5, 19, 3.2, 3.2, "B")
    g.ell(5, 19, 2.0, 2.0, "b")
    if f == 0:
        g.line(5, 17, 5, 21, "B"); g.line(3, 19, 7, 19, "B")
    else:
        g.line(4, 18, 6, 20, "B"); g.line(6, 18, 4, 20, "B")
    g.px(5, 19, "k")
    arm(g, cx - 5, 13, cx - 6, 14, "o")
    arm(g, cx + 4, 13, cx + 5, 15, "O")
    return g


CHARS = [
    ("bookie", bookie, "The Bookie"),
    ("inker", inker, "The Inker"),
    ("iclerk", iclerk, "The Inker's clerk"),
    ("penciller", penciller, "The Penciller"),
    ("pclerk", pclerk, "The Penciller's clerk"),
    ("doorman", doorman, "The doorman (vnode proxy)"),
    ("movers", movers, "The movers (handoff)"),
]


def build():
    out = {}
    for name, fn, _ in CHARS:
        frames = []
        for f in (0, 1):
            g = fn(f)
            g.outline()
            frames.append(g.rows())
        out[name] = frames
    return out


def preview(sprites, path, scale=8):
    cols = len(sprites)
    img = Image.new("RGB", (cols * (W + 2) * scale, 2 * (H + 2) * scale), "#efe6f3")
    for ci, (name, frames) in enumerate(sprites.items()):
        for fi, rows in enumerate(frames):
            ox = (ci * (W + 2) + 1) * scale
            oy = (fi * (H + 2) + 1) * scale
            for y, row in enumerate(rows):
                for x, c in enumerate(row):
                    if c == ".":
                        continue
                    col = PAL[c][1]
                    for dy in range(scale):
                        for dx in range(scale):
                            img.putpixel((ox + x * scale + dx, oy + y * scale + dy), tuple(int(col[i:i + 2], 16) for i in (1, 3, 5)))
    img.save(path)


def write_ts(sprites, path):
    lines = [
        "// Generated by scripts/draw-characters.py; edit that, not this. The leveled characters and",
        "// the ones around the vnodes, 24 by 24 pixels, two frames each: idle,",
        "// and at work. Each letter is a colour from PALETTE; \".\" is empty.",
        "",
        "export type Frame = readonly string[];",
        "",
        "/** Letter to colour token. The tokens are fixed in both themes, because",
        " *  the characters stand on their own paper stage. */",
        "export const PALETTE: Record<string, string> = {",
    ]
    for k, (tok, _) in PAL.items():
        lines.append(f'  {json.dumps(k)}: "var(--pix-{tok})",')
    lines.append("};")
    lines.append("")
    for name, frames in sprites.items():
        lines.append(f"export const {name}: [Frame, Frame] = [")
        for rows in frames:
            lines.append("  [")
            for r in rows:
                lines.append(f'    "{r}",')
            lines.append("  ],")
        lines.append("];")
        lines.append("")
    open(path, "w").write("\n".join(lines))


def write_tokens(path):
    lines = ["  /* Pixel character colours (characters.ts): the same in both themes. */"]
    for k, (tok, hexv) in PAL.items():
        lines.append(f"  --pix-{tok}: {hexv};")
    open(path, "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    s = build()
    preview(s, sys.argv[1])
    if len(sys.argv) > 2:
        write_ts(s, sys.argv[2])
    if len(sys.argv) > 3:
        write_tokens(sys.argv[3])
